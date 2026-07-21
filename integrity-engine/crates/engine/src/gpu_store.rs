//! **The one GPU particle container** (`docs/50`) — allocation, capacity/count bookkeeping, and the
//! two-phase asynchronous read-back, shared by every GPU particle pipeline.
//!
//! `gpu_particles` (granular) and `gpu_sph` (SPH) each carried their own copy of all three. The
//! read-backs were byte-for-byte identical apart from the element type and a debug label, and the proof
//! that this was duplication rather than coincidence is that the SAME latent defect — an
//! `Rc<Cell<bool>>` in the `map_async` callback, which compiles only for wasm — had to be found and
//! fixed twice, once in each file, on the same day.
//!
//! **The solvers are NOT unified, and that is not a dodge.** `docs/46` §1 sanctions their separation:
//! stiff granular contacts need a semi-implicit integrator, self-gravitating SPH needs a symplectic
//! leapfrog, and forcing one on both is unstable or ruinously slow — the *physics* differs, so the
//! numerics differ. What was never physics is the allocator: "how much room is left, where does the next
//! batch land, and how do I get it back without blocking" has one right answer.
//!
//! Generic over the element so an 80-byte granular grain and a 48-byte SPH particle share the code
//! without sharing a layout.

use bytemuck::Pod;

/// How many of `incoming` fit after `count`, and the ELEMENT offset they land at.
///
/// Pure so it is testable natively — wgpu is built here with the `webgpu` backend only, so a
/// `ParticleStore` cannot be instantiated off-browser, but this arithmetic can. It is also where the
/// only silent bug lives: an off-by-one at the capacity boundary drops particles with no error, which is
/// matter vanishing, and no rendering check would catch it.
pub(crate) fn append_span(count: u32, capacity: u32, incoming: usize) -> (usize, u64) {
    let room = capacity.saturating_sub(count) as usize;
    (incoming.min(room), count as u64)
}

/// How many of `incoming` fit when replacing the whole contents. Always writes at element 0.
pub(crate) fn replace_span(capacity: u32, incoming: usize) -> usize {
    incoming.min(capacity as usize)
}

/// A GPU-resident particle array plus its non-blocking read-back.
pub(crate) struct ParticleStore<T: Pod> {
    buf: wgpu::Buffer,
    capacity: u32,
    count: u32,
    label: &'static str,
    /// Set by the `map_async` callback when the staging map completes. `Arc<AtomicBool>`, not
    /// `Rc<Cell<bool>>`: wgpu bounds that callback by `WasmNotSend`, a no-op on wasm but plain `Send`
    /// everywhere else, so the `Rc` form compiles ONLY for wasm. Release/Acquire because the callback
    /// publishes a completed mapping that `take_readback` then reads through `get_mapped_range`.
    ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
    staging: Option<wgpu::Buffer>,
    /// `count` at the moment the copy was recorded. A caller compares it against the live count to
    /// detect an append that landed mid-flight and discard the now-misaligned snapshot rather than
    /// deposit stale data.
    readback_count: u32,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Pod> ParticleStore<T> {
    /// `extra_usage` carries whatever the solver additionally needs (e.g. `VERTEX` to draw straight from
    /// the physics buffer). `COPY_SRC` is always present so the read-back copy is legal — omitting it
    /// makes `copy_buffer_to_buffer` a silent, asynchronous WebGPU validation error.
    pub(crate) fn new(
        device: &wgpu::Device,
        capacity: u32,
        extra_usage: wgpu::BufferUsages,
        label: &'static str,
    ) -> Self {
        let buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: capacity as u64 * std::mem::size_of::<T>() as u64,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_DST
                | wgpu::BufferUsages::COPY_SRC
                | extra_usage,
            mapped_at_creation: false,
        });
        ParticleStore {
            buf,
            capacity,
            count: 0,
            label,
            ready: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false)),
            staging: None,
            readback_count: 0,
            _marker: std::marker::PhantomData,
        }
    }

    pub(crate) fn buffer(&self) -> &wgpu::Buffer {
        &self.buf
    }
    pub(crate) fn count(&self) -> u32 {
        self.count
    }
    pub(crate) fn capacity(&self) -> u32 {
        self.capacity
    }
    pub(crate) fn readback_count(&self) -> u32 {
        self.readback_count
    }
    pub(crate) fn set_count(&mut self, n: u32) {
        self.count = n.min(self.capacity);
    }

    /// Append after the live particles, clamped to remaining room. Overflow is DROPPED (flagged).
    pub(crate) fn append(&mut self, queue: &wgpu::Queue, new: &[T]) {
        let (take, at) = append_span(self.count, self.capacity, new.len());
        if take == 0 {
            return;
        }
        let offset = at * std::mem::size_of::<T>() as u64;
        queue.write_buffer(&self.buf, offset, bytemuck::cast_slice(&new[..take]));
        self.count += take as u32;
    }

    /// Replace the whole contents (the survivors after a de-resolution pass, or a fresh upload).
    pub(crate) fn replace(&mut self, queue: &wgpu::Queue, items: &[T]) {
        let take = replace_span(self.capacity, items.len());
        if take > 0 {
            queue.write_buffer(&self.buf, 0, bytemuck::cast_slice(&items[..take]));
        }
        self.count = take as u32;
    }

    /// Phase 1: copy the live particles into a MAP_READ staging buffer and start the async map. No-op if
    /// empty or already in flight. WebGPU maps cannot block (`Maintain::Wait` is a no-op in the browser),
    /// so the result is collected a later frame by [`Self::take_readback`].
    pub(crate) fn begin_readback(&mut self, device: &wgpu::Device, queue: &wgpu::Queue) {
        if self.count == 0 || self.staging.is_some() {
            return;
        }
        let size = self.count as u64 * std::mem::size_of::<T>() as u64;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(self.label),
            size,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        enc.copy_buffer_to_buffer(&self.buf, 0, &staging, 0, size);
        queue.submit(std::iter::once(enc.finish()));
        self.ready.store(false, std::sync::atomic::Ordering::Release);
        let flag = self.ready.clone();
        staging.slice(..).map_async(wgpu::MapMode::Read, move |res| {
            if res.is_ok() {
                flag.store(true, std::sync::atomic::Ordering::Release);
            }
        });
        self.readback_count = self.count;
        self.staging = Some(staging);
    }

    /// Phase 2: the snapshot if the map completed, else `None` while pending or idle.
    pub(crate) fn take_readback(&mut self) -> Option<Vec<T>> {
        if !self.ready.load(std::sync::atomic::Ordering::Acquire) {
            return None;
        }
        let staging = self.staging.take()?;
        let data = staging.slice(..).get_mapped_range();
        let out = bytemuck::cast_slice::<u8, T>(&data).to_vec();
        drop(data);
        staging.unmap();
        self.ready.store(false, std::sync::atomic::Ordering::Release);
        Some(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The capacity boundary is the whole risk: over-taking corrupts memory past the buffer, under-taking
    /// silently drops matter. Neither surfaces as an error at runtime.
    #[test]
    fn append_clamps_to_the_room_that_is_actually_left() {
        assert_eq!(append_span(0, 100, 10), (10, 0), "empty store takes all, at 0");
        assert_eq!(append_span(90, 100, 10), (10, 90), "exactly fills");
        assert_eq!(append_span(95, 100, 10), (5, 95), "partial: only the room left");
        assert_eq!(append_span(100, 100, 10), (0, 100), "full store takes nothing");
        assert_eq!(append_span(0, 0, 10), (0, 0), "zero-capacity store takes nothing");
        // A count past capacity must not underflow into a huge `room` (saturating_sub is load-bearing).
        assert_eq!(append_span(120, 100, 10), (0, 120), "over-full cannot wrap to enormous room");
    }

    #[test]
    fn replace_clamps_to_capacity() {
        assert_eq!(replace_span(100, 10), 10);
        assert_eq!(replace_span(100, 100), 100);
        assert_eq!(replace_span(100, 250), 100, "excess is dropped, not written past the end");
        assert_eq!(replace_span(0, 5), 0);
    }

    /// Appending in pieces must land exactly where appending once would, or particles overwrite each
    /// other — matter silently disappearing rather than a crash.
    #[test]
    fn successive_appends_tile_without_gap_or_overlap() {
        let (cap, mut count) = (100u32, 0u32);
        let mut spans = Vec::new();
        for batch in [30usize, 30, 30, 30] {
            let (take, at) = append_span(count, cap, batch);
            spans.push((at, take));
            count += take as u32;
        }
        assert_eq!(spans, vec![(0, 30), (30, 30), (60, 30), (90, 10)]);
        assert_eq!(count, cap, "the store ends exactly full, never over");
        // Every element index is covered exactly once.
        let mut covered = vec![0u8; cap as usize];
        for (at, take) in spans {
            for i in at as usize..at as usize + take {
                covered[i] += 1;
            }
        }
        assert!(covered.iter().all(|&c| c == 1), "gap or overlap in the tiling");
    }
}
