//! PSI section assembler for multi-packet sections.
//!
//! DVB/MPEG-TS PSI tables can exceed a single TS packet payload (~183 bytes).
//! This module buffers section data across continuation packets and returns
//! complete sections ready for parsing.

const MAX_SECTION_SIZE: usize = 4096;

pub struct SectionAssembler {
    buf: Vec<u8>,
    expected: Option<usize>, // 3 + section_length
}

impl SectionAssembler {
    pub fn new() -> Self {
        Self {
            buf: Vec::with_capacity(256),
            expected: None,
        }
    }

    /// Feed a TS packet payload to the assembler.
    ///
    /// Returns a Vec of complete raw section byte slices (from table_id through CRC).
    /// Each returned Vec<u8> is prepended with a `0x00` pointer_field byte so
    /// existing `SectionReader`-based parsers work unchanged.
    pub fn push(&mut self, payload: &[u8], payload_unit_start: bool) -> Vec<Vec<u8>> {
        let mut complete = Vec::new();

        if payload_unit_start {
            // The first byte is the pointer_field
            if payload.is_empty() {
                self.reset();
                return complete;
            }
            let pointer = payload[0] as usize;
            let after_pointer = 1 + pointer;

            // If there's a pointer > 0, the bytes between [1..after_pointer]
            // are the tail of the previous section
            if pointer > 0 && after_pointer <= payload.len() {
                self.buf.extend_from_slice(&payload[1..after_pointer]);
                if let Some(section) = self.try_complete() {
                    complete.push(section);
                }
            }

            // Reset for the new section(s) starting at after_pointer
            self.reset();

            let mut pos = after_pointer;
            while pos < payload.len() {
                // Skip 0xFF stuffing bytes
                if payload[pos] == 0xFF {
                    break;
                }

                // Start a new section accumulation
                let remaining = &payload[pos..];
                self.buf.extend_from_slice(remaining);

                if let Some(section) = self.try_complete() {
                    // Section completed within this packet; there might be
                    // another section following it (though rare for PSI).
                    let consumed = section.len() - 1; // minus the prepended pointer byte
                    complete.push(section);
                    self.reset();
                    pos += consumed;
                } else {
                    // Section spans more packets; compute expected length
                    if self.buf.len() >= 3 {
                        let sec_len = (((self.buf[1] & 0x0F) as usize) << 8)
                            | (self.buf[2] as usize);
                        let total = 3 + sec_len;
                        if total > MAX_SECTION_SIZE {
                            self.reset();
                        } else {
                            self.expected = Some(total);
                        }
                    }
                    break;
                }
            }
        } else {
            // Continuation packet — append to current buffer
            if self.expected.is_some() {
                self.buf.extend_from_slice(payload);
                if let Some(section) = self.try_complete() {
                    complete.push(section);
                    self.reset();
                } else if self.buf.len() > MAX_SECTION_SIZE {
                    self.reset(); // safety limit
                }
            }
            // If no section in progress, drop the payload (orphan continuation)
        }

        complete
    }

    /// Check if the buffer contains a complete section. If so, extract it
    /// prepended with a 0x00 pointer_field and return it.
    fn try_complete(&mut self) -> Option<Vec<u8>> {
        if self.buf.len() < 3 {
            return None;
        }
        let sec_len = (((self.buf[1] & 0x0F) as usize) << 8) | (self.buf[2] as usize);
        let total = 3 + sec_len;

        if total > MAX_SECTION_SIZE {
            return None;
        }

        if self.buf.len() >= total {
            // Prepend 0x00 pointer_field for SectionReader compatibility
            let mut out = Vec::with_capacity(1 + total);
            out.push(0x00);
            out.extend_from_slice(&self.buf[..total]);
            // Keep any leftover bytes (for back-to-back sections in same payload)
            // Actually, we handle that in push(), so just note the total consumed.
            self.buf.drain(..total);
            Some(out)
        } else {
            self.expected = Some(total);
            None
        }
    }

    fn reset(&mut self) {
        self.buf.clear();
        self.expected = None;
    }
}
