//! **The renderer, as a thing you can ASK** (docs/68 step 3, first increment).
//!
//! Robin's division: *"the renderer is a separate entity from the core engine; perhaps its realm is
//! light and it handles the raytracing, since particle physics cares little about photons."* The
//! extraction proper is a protocol and, eventually, its own thread or process. This is the piece that
//! has to come first, and the reason is measured rather than argued.
//!
//! ## Why observability is the first thing, not the last
//!
//! Bisecting one defect — Terra's flora meshed at 473,760 indices and producing **zero fragments** —
//! took **nine deploy-and-photograph cycles**, because a photograph was the only instrument available.
//! The engine's `wgpu` is pinned to the webgpu backend, so nothing in-process can make a native device;
//! `tools/gpu-verify` and `tools/sph-verify` are standalone crates with their own `wgpu` and can only
//! run REPLICAS of the shipped code. **The engine could not look at its own GPU state.**
//!
//! Robin, watching that: *"remember my hint about separating the renderer from the engine itself; I
//! think that will add observability opportunities that can help resolve this type of situation
//! faster."* Correct, and under-weighted by me — `docs/68` §6b now carries the count.
//!
//! So the first thing the renderer gains is the ability to answer **"what do you actually hold?"** —
//! about the device, not about the intention.
//!
//! ## The asymmetry this closes
//!
//! Every upload in this engine is a one-way statement: build bytes, hand them to `write_buffer`, hope.
//! [`Uploaded`] records what was *stated* at the moment it was stated, and [`Readback`] fetches what
//! the device *has*. Comparing them is the check that could not be run — and the flora defect is
//! bisected to exactly that comparison: uniform, pipeline and draw all proven good, contents unproven.
//!
//! ## Why a readback is deferred rather than immediate
//!
//! `wgpu::BufferSlice::map_async` completes on the device's own schedule, and in a browser there is no
//! blocking wait — `Device::poll` cannot stall a frame there. So a readback is **requested on one frame
//! and collected on a later one**, which is not a limitation to work around but the honest shape of
//! asking another processor a question. It is also exactly the shape the eventual cross-thread protocol
//! needs, which is why building it now is not throwaway.

use std::sync::{Arc, Mutex};

/// **What was uploaded, recorded where it was uploaded.** Half of the comparison; [`Readback`] is the
/// other half.
///
/// The checksum is a plain sum over bytes rather than anything cryptographic: the question is *"is this
/// the data we sent, or is it zeros/stale?"*, and for that a sum that a buffer of zeros cannot fake is
/// enough. `bytes` and `nonzero` are carried alongside because a zero checksum is ambiguous and a zero
/// `nonzero` count is not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Uploaded {
    pub bytes: usize,
    pub checksum: u64,
    /// How many of those bytes were not zero — the statistic that distinguishes "never written" from
    /// "written with something that happens to sum oddly".
    pub nonzero: usize,
}

impl Uploaded {
    /// Describe a slice of bytes at the moment they are handed to the device.
    pub fn of(bytes: &[u8]) -> Uploaded {
        let mut checksum = 0u64;
        let mut nonzero = 0usize;
        for (i, b) in bytes.iter().enumerate() {
            if *b != 0 {
                nonzero += 1;
                // Position-sensitive, so a permutation is not mistaken for a match.
                checksum = checksum
                    .wrapping_mul(0x0100_0000_01B3)
                    .wrapping_add(*b as u64 ^ (i as u64));
            }
        }
        Uploaded {
            bytes: bytes.len(),
            checksum,
            nonzero,
        }
    }

    /// Does the device hold what we said we sent?
    pub fn matches(&self, other: &Uploaded) -> bool {
        self.bytes == other.bytes
            && self.checksum == other.checksum
            && self.nonzero == other.nonzero
    }

    /// A buffer that was never written reads as this — all zeros, whatever its size.
    pub fn is_blank(&self) -> bool {
        self.nonzero == 0
    }
}

/// A request for what the device holds, delivered later. See the module docs for why it is deferred.
///
/// One request at a time per instance: a second [`Self::request`] before the first is collected
/// replaces it, because the interesting question is always about the current contents.
pub struct Readback {
    staging: Option<wgpu::Buffer>,
    /// Set by the map callback, which runs on the device's schedule and not on ours.
    landed: Arc<Mutex<Option<Uploaded>>>,
    /// What the CPU said it sent, kept beside the request so the comparison needs nothing else.
    claimed: Uploaded,
    label: String,
}

impl Readback {
    /// Ask the device for `bytes` from the start of `source`. `source` must carry `COPY_SRC`.
    ///
    /// The copy is recorded into `encoder`, so it happens in the same submission as the frame — which
    /// means the answer is about the buffer as the frame saw it, not as it was some time afterwards.
    pub fn request(
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        label: &str,
        source: &wgpu::Buffer,
        bytes: u64,
        claimed: Uploaded,
    ) -> Readback {
        // COPY_BUFFER_ALIGNMENT: a mapped read has to start and end on 4 bytes.
        let size = (bytes / 4) * 4;
        let staging = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size.max(4),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        encoder.copy_buffer_to_buffer(source, 0, &staging, 0, size.max(4));
        Readback {
            staging: Some(staging),
            landed: Arc::new(Mutex::new(None)),
            claimed,
            label: label.to_string(),
        }
    }

    /// Start the map. Call once, after the submission that carries the copy — mapping a buffer whose
    /// copy has not been submitted is a request about nothing.
    pub fn begin(&self) {
        let Some(staging) = self.staging.as_ref() else {
            return;
        };
        let landed = Arc::clone(&self.landed);
        let slice = staging.slice(..);
        let buf = staging.clone();
        slice.map_async(wgpu::MapMode::Read, move |res| {
            if res.is_ok() {
                let view = buf.slice(..).get_mapped_range();
                *landed.lock().unwrap() = Some(Uploaded::of(&view));
                drop(view);
                buf.unmap();
            }
        });
    }

    /// Has the answer arrived? `None` until the device delivers it.
    pub fn collect(&self) -> Option<Uploaded> {
        *self.landed.lock().unwrap()
    }

    /// **The comparison this type exists for**, as a line a human can read. `None` until the answer
    /// arrives.
    pub fn verdict(&self) -> Option<String> {
        let held = self.collect()?;
        Some(if held.is_blank() && !self.claimed.is_blank() {
            format!(
                "{}: THE DEVICE HOLDS NOTHING — {} bytes claimed with {} non-zero, {} bytes read all zero. \
                 The upload did not land.",
                self.label, self.claimed.bytes, self.claimed.nonzero, held.bytes
            )
        } else if self.claimed.matches(&held) {
            format!(
                "{}: the device holds what was sent ({} bytes, {} non-zero)",
                self.label, held.bytes, held.nonzero
            )
        } else {
            format!(
                "{}: MISMATCH — sent {} bytes/{} non-zero/sum {:x}, device has {} bytes/{} non-zero/sum {:x}",
                self.label,
                self.claimed.bytes,
                self.claimed.nonzero,
                self.claimed.checksum,
                held.bytes,
                held.nonzero,
                held.checksum
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// **A blank buffer is distinguishable from a written one, and that is the whole point.** The flora
    /// defect's signature is a correct index count over vertices that are all zero — every triangle
    /// degenerate, zero fragments — and `nonzero` is what tells those apart when a checksum alone
    /// cannot.
    #[test]
    fn nothing_uploaded_reads_as_nothing_and_says_so() {
        let real = Uploaded::of(&[1u8, 0, 3, 9, 0, 200]);
        let blank = Uploaded::of(&[0u8; 6]);
        assert!(!real.is_blank());
        assert!(blank.is_blank(), "all zeros is blank whatever its length");
        assert!(!real.matches(&blank));
        assert_eq!(real.bytes, blank.bytes, "same size, different contents");
    }

    /// **Position-sensitive**, so a permutation is not mistaken for a match — a buffer written with the
    /// right bytes in the wrong order is a different defect and must read as one.
    #[test]
    fn the_same_bytes_in_a_different_order_do_not_match() {
        let a = Uploaded::of(&[1u8, 2, 3, 4]);
        let b = Uploaded::of(&[4u8, 3, 2, 1]);
        assert_eq!(a.nonzero, b.nonzero);
        assert_ne!(a.checksum, b.checksum, "order must change the answer");
        assert!(!a.matches(&b));
    }

    /// And the honest positive: the same bytes match themselves.
    #[test]
    fn what_was_sent_matches_itself() {
        let bytes: Vec<u8> = (0..512u32).map(|i| (i % 251) as u8).collect();
        assert!(Uploaded::of(&bytes).matches(&Uploaded::of(&bytes)));
    }
}
