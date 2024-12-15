// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! Modules containing packet definitions

pub mod baseline;
pub mod extended;
pub mod service_mode;

pub use baseline::*;
pub use service_mode::*;

use crate::Error;
use bitvec::prelude::*;

/// An ID corresponding to the address of a decoder
pub type Address = u8;

/// Represents a generic Operations Mode packet that can be transmitted or received
pub trait Packet {
    /// Serialize the packet
    ///
    /// # Returns
    ///
    /// Returns the total bits needed to transmit the packet if `Ok`, otherwise [`Error`]s.
    fn serialize(data: &[u8], buf: &mut SerializeBuffer) -> Result<usize> {
        // check that the provided data will fit into the buffer
        let required_bits = 15 + data.len() * 9 + 1;
        if required_bits > MAX_BITS {
            return Err(Error::TooLong);
        }

        buf[0..16].copy_from_bitslice([0xff, 0xfe].view_bits::<Msb0>()); // preamble

        let mut pos: usize = 15; // move after preamble
        for byte in data {
            buf.set(pos, false); // If first iteration, right after preamble. Marks the start of a new byte
            pos += 1;
            buf[pos..pos + 8].copy_from_bitslice([*byte].view_bits::<Msb0>());
            pos += 8; // Move to the next 8 bits in the loop // TODO: Can we chunk this with iterators instead of this imperative code?
        }

        buf.set(pos, true); // stop bit
        pos += 1; // Move the position to after the stop bit to capture the correct amount of bits in our result

        Ok(pos)
    }
}

/// Convenient Result wrapper
pub type Result<T> = core::result::Result<T, Error>;

// TODO: Controller preambles must send a minimum of 14 bits whereas receivers only require 10
struct Preamble(BitArr!(for 14, in u8, Msb0));

// The size of our buffer, which should be long enough to serialize any common DCC packet into
const MAX_BITS: usize = 15 + 4 * 9 + 1;

/// A buffer that is [`MAX_BITS`] long, which is enough to serialize any common DCC packet info
pub type SerializeBuffer = BitArr!(for MAX_BITS, in u8, Msb0);

#[cfg(test)]
mod test {
    use super::*;

    pub fn print_chunks(buf: &SerializeBuffer, limit: usize) {
        println!("Preamble: {}", &buf[..15]);

        let mut offset = 15;
        while offset < limit - 1 {
            println!(
                "[{}] Chunk: {}-{:08b}",
                offset,
                buf[offset] as u8,
                &buf[offset + 1..offset + 9]
            );
            offset += 9;
        }
        println!("Stop bit: {}", buf[offset] as u8);
    }
}
