// This Source Code Form is subject to the terms of the Mozilla Public
// License, v. 2.0. If a copy of the MPL was not distributed with this
// file, You can obtain one at https://mozilla.org/MPL/2.0/.

//! This module provides types and serializers for each "baseline"
//! packet type defined by the NMRA standard.
//!
//! <https://www.nmra.org/sites/default/files/s-92-2004-07.pdf>

use core::ops::Not;

use super::{Address, Packet, Preamble, Result, SerializeBuffer};
use crate::Error;
use bitvec::prelude::*;

impl Default for Preamble {
    fn default() -> Self {
        // 16 total "1" bits
        Self(BitArray::from([0xff, 0xff]))
    }
}

/// Possible directions, usually referenced to the "forward" direction
/// of a loco
#[derive(Debug, Default, Copy, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "use-defmt", derive(defmt::Format))]
pub enum Direction {
    /// Forward
    #[default]
    Forward,
    /// Backward
    Backward,
}

impl Not for Direction {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            Direction::Forward => Direction::Backward,
            Direction::Backward => Direction::Forward,
        }
    }
}

/// Speed and Direction packet. Used to command a loco to move in the
/// given direction at the given speed.
///
/// The speed part of the instruction is five bits wide, with the bits
/// ordered `04321`, where `0` is LSB and `4` is MSB. The speed
/// instructions are defined by the following list:
/// ```ignore
///  0 4321 | meaning
///  ---------------------------------------------
///  0 0000 | stop
///  1 0000 | also stop
///  0 0001 | e-stop
///  1 0001 | also e-stop
///  0 0010 | speed 1 (0x04)
///   ...   |   ...
///  1 1111 | speed 28 (0x1f)
/// ```
pub struct SpeedAndDirection {
    address: Address,
    instruction: u8,
}

impl Packet for SpeedAndDirection {}

impl SpeedAndDirection {
    /// Builder interface for `SpeedAndDirection`. Use of the Builder
    /// pattern ensures that only valid packets are produced.
    pub fn builder() -> SpeedAndDirectionBuilder {
        SpeedAndDirectionBuilder::default()
    }

    /// Serialize the packed into the provided buffer
    pub fn serialize(&self, buf: &mut SerializeBuffer) -> Result<usize> {
        <Self as Packet>::serialize(
            &[
                self.address,
                self.instruction,
                self.address ^ self.instruction,
            ],
            buf,
        )
    }
}

/// Builder used to construct a SpeedAndDirection packet
#[derive(Default)]
pub struct SpeedAndDirectionBuilder {
    address: Option<Address>,
    speed: Option<u8>,
    e_stop: bool,
    direction: Option<Direction>,
}

impl SpeedAndDirectionBuilder {
    /// Sets the address. In short mode, this must be between 1
    /// and 126. Returns [`Error::InvalidAddress`] if the provided address
    /// is outside of this range.
    pub fn address(&mut self, address: Address) -> Result<&mut Self> {
        if address == 0 || address > 0x7f {
            Err(Error::InvalidAddress)
        } else {
            self.address = Some(address);
            Ok(self)
        }
    }

    /// Sets the speed. In short mode the speed has to be between 0 and
    /// 16. Returns [`Error::InvalidSpeed`] if the provided speed is outside
    /// this range.
    pub fn speed(&mut self, speed: u8) -> Result<&mut Self> {
        if speed > 28 {
            Err(Error::InvalidSpeed)
        } else {
            self.speed = Some(speed);
            Ok(self)
        }
    }

    /// Sets the direction
    pub fn direction(&mut self, direction: Direction) -> &mut Self {
        self.direction = Some(direction);
        self
    }

    /// Sends the e-stop signal. Overrides any other set speed value
    pub fn e_stop(&mut self, e_stop: bool) -> &mut Self {
        self.e_stop = e_stop;
        self
    }

    /// Build a [`SpeedAndDirection`] packet using the provided values,
    /// falling back to sensible defaults if not all fields have been
    /// provided.
    ///
    /// # Defaults
    ///
    /// * `speed = 0`
    /// * `direction = Forward`
    /// * `address = 3`
    /// * `headlight = false`
    pub fn build(&mut self) -> SpeedAndDirection {
        let address: Address = self.address.unwrap_or(3);
        // add the weird offset to the speed
        let speed = match self.speed {
            Some(0) | None => 0,
            Some(speed) => speed + 3,
        };
        #[cfg(test)]
        eprintln!("Speed is {speed} = {speed:08b}");
        let mut instruction = 0b0100_0000; // packet type
        if let Direction::Forward = self.direction.unwrap_or_default() {
            instruction |= 0b0010_0000;
        }

        // e-stop overrides other speed setting
        if self.e_stop {
            instruction |= 0x01;
        } else {
            // upper four bits of speed
            instruction |= (speed >> 1) & 0x0f;

            // LSB of speed
            instruction |= (speed & 0x01) << 4;
        }

        SpeedAndDirection {
            address,
            instruction,
        }
    }
}

/// A Reset packet is one in which the address, instruction, and ECC are
/// all zero. All decoders will, upon receiving this packet, reset to their
/// normal power-up state. Any speed or direction will be cleared and
/// locomotives stopped.
pub struct Reset;

impl Packet for Reset {}

impl Reset {
    /// Serialize the packed into the provided buffer
    pub fn serialize(&self, buf: &mut SerializeBuffer) -> Result<usize> {
        <Self as Packet>::serialize(&[0x00, 0x00, 0x00], buf)
    }
}

/// An Idle packet is one in which the address is 0xff and instruction 0x00.
/// Upon receiving this, a decoder performs no new action.
pub struct Idle;

impl Packet for Idle {}

impl Idle {
    /// Serialize the packed into the provided buffer
    pub fn serialize(&self, buf: &mut SerializeBuffer) -> Result<usize> {
        <Self as Packet>::serialize(&[0xff, 0x00, 0xff], buf)
    }
}

/// Instruct all decoders to stop, either immediately or by simply
/// removing motor power ("float"). The specification allows a direction
/// field, but that is not implemented here because in situations where
/// a BroadcastStop is being sent it is unlikely that direction settings
/// will be important. (whereas a regular stop might wish to retain
/// headlight states)
pub struct BroadcastStop {
    float: bool,
}

impl Packet for BroadcastStop {}

impl BroadcastStop {
    /// Bring all locomotives to an immediate stop
    pub fn immediate() -> Self {
        Self { float: false }
    }

    /// Bring all locomotives to a gently/floating stop
    pub fn float() -> Self {
        Self { float: true }
    }

    /// Serialize the packed into the provided buffer
    pub fn serialize(&self, buf: &mut SerializeBuffer) -> Result<usize> {
        let instr = if self.float { 0b0101_0000 } else { 0b0100_0000 };

        <Self as Packet>::serialize(&[0x00, instr, instr], buf)
    }
}

#[cfg(test)]
mod test {
    use super::*;

    fn display_serialize_buffer(buf: &SerializeBuffer) {
        println!("{buf:?}");
        //        15              1 8        1 8        1 8        1
        //        15              16 24      25 33      34 42      43
        println!("ppppppppppppppp s aaaaaaaa s 01dvvvvv s cccccccc s");
        println!(
            "{} {} {} {} {} {} {} {}",
            buf[..15]
                .iter()
                .map(|b| if *b { "1" } else { "0" })
                .collect::<Vec<_>>()
                .join(""),
            if *buf.get(15).unwrap() { "1" } else { "0" },
            buf[16..24]
                .iter()
                .map(|b| if *b { "1" } else { "0" })
                .collect::<Vec<_>>()
                .join(""),
            if *buf.get(24).unwrap() { "1" } else { "0" },
            buf[25..33]
                .iter()
                .map(|b| if *b { "1" } else { "0" })
                .collect::<Vec<_>>()
                .join(""),
            if *buf.get(33).unwrap() { "1" } else { "0" },
            buf[34..42]
                .iter()
                .map(|b| if *b { "1" } else { "0" })
                .collect::<Vec<_>>()
                .join(""),
            if *buf.get(42).unwrap() { "1" } else { "0" },
        );
    }

    #[test]
    fn make_speed_and_direction() -> Result<()> {
        let pkt = SpeedAndDirection::builder()
            .address(35)?
            .speed(14)?
            .direction(Direction::Forward)
            .build();
        assert_eq!(pkt.address, 35);
        let expected = 0b0111_1000;
        eprintln!("Got instruction: {:08b}", pkt.instruction);
        eprintln!("Expected:        {expected:08b}");
        assert_eq!(pkt.instruction, expected);

        Ok(())
    }

    #[test]
    fn serialize_speed_and_direction() -> Result<()> {
        let pkt = SpeedAndDirection::builder()
            .address(35)?
            .speed(14)?
            .direction(Direction::Forward)
            .build();
        let mut buf = SerializeBuffer::default();
        let len = pkt.serialize(&mut buf)?;
        // instruction is:
        // 01 D S SSSS
        // 01 1 1 1101
        #[allow(clippy::unusual_byte_groupings)]
        let expected_arr = [
            0xff_u8,      // preamble
            0b1111_1110,  // preamble + start
            35,           // address
            0b0_0111_100, // start + instr[..7]
            0b0_0_010110, // instr[7] + start + ecc[..6]
            0b11_1_00000, // ecc[6..] + stop + 5 zeroes
        ];
        let mut expected = SerializeBuffer::default();
        expected[..43]
            .copy_from_bitslice(&expected_arr.view_bits::<Msb0>()[..43]);
        println!("got:");
        display_serialize_buffer(&buf);
        println!("expected:");
        display_serialize_buffer(&expected);
        assert_eq!(len, 43);
        assert_eq!(buf[..len], expected[..43]);
        Ok(())
    }

    #[test]
    fn serialize_reset_packet() -> Result<()> {
        let pkt = Reset;
        let mut buf = SerializeBuffer::default();
        let len = pkt.serialize(&mut buf)?;

        #[allow(clippy::unusual_byte_groupings)]
        let expected_arr = [
            0xff_u8,      // preamble
            0b1111_1110,  // preamble + start
            0x00,         // address
            0b0_0000_000, // start + instr[..7]
            0b0_0_000000, // instr[7] + start + ecc[..6]
            0b00_1_00000, // ecc[6..] + stop + 5 zeroes
        ];

        let mut expected = SerializeBuffer::default();
        expected[..43]
            .copy_from_bitslice(&expected_arr.view_bits::<Msb0>()[..43]);
        println!("got:");
        display_serialize_buffer(&buf);
        println!("expected:");
        display_serialize_buffer(&expected);
        assert_eq!(len, 43);
        assert_eq!(buf[..len], expected[..43]);
        Ok(())
    }

    #[test]
    fn serialize_idle_packet() -> Result<()> {
        let pkt = Idle;
        let mut buf = SerializeBuffer::default();
        let len = pkt.serialize(&mut buf)?;

        #[allow(clippy::unusual_byte_groupings)]
        let expected_arr = [
            0xff_u8,      // preamble
            0b1111_1110,  // preamble + start
            0xff,         // address
            0b0_0000_000, // start + instr[..7]
            0b0_0_111111, // instr[7] + start + ecc[..6]
            0b11_1_00000, // ecc[6..] + stop + 5 zeroes
        ];

        let mut expected = SerializeBuffer::default();
        expected[..43]
            .copy_from_bitslice(&expected_arr.view_bits::<Msb0>()[..43]);
        println!("got:");
        display_serialize_buffer(&buf);
        println!("expected:");
        display_serialize_buffer(&expected);
        assert_eq!(len, 43);
        assert_eq!(buf[..len], expected[..43]);
        Ok(())
    }

    #[test]
    fn serialize_broadcast_stop_packet() -> Result<()> {
        let pkt = BroadcastStop::float();
        let mut buf = SerializeBuffer::default();
        let len = pkt.serialize(&mut buf)?;

        #[allow(clippy::unusual_byte_groupings)]
        let expected_arr = [
            0xff_u8,      // preamble
            0b1111_1110,  // preamble + start
            0x00,         // address
            0b0_0101_000, // start + instr[..7]
            0b0_0_010100, // instr[7] + start + ecc[..6]
            0b00_1_00000, // ecc[6..] + stop + 5 zeroes
        ];

        let mut expected = SerializeBuffer::default();
        expected[..43]
            .copy_from_bitslice(&expected_arr.view_bits::<Msb0>()[..43]);
        println!("got:");
        display_serialize_buffer(&buf);
        println!("expected:");
        display_serialize_buffer(&expected);
        assert_eq!(len, 43);
        assert_eq!(buf[..len], expected[..43]);
        Ok(())
    }
}
