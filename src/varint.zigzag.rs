use std::cmp::PartialEq;
use std::mem;
use std::io::{Error, Write};
trait ZigZag<T> {
  fn zigzag(self) -> T;
}

macro_rules! impl_zigzag {
    ($IT:ty, $UT:ty) => {
        impl ZigZag<$IT> for $UT {
            fn zigzag(self) -> $IT {
                ((self >> 1) ^ (-((self & 1) as $IT)) as $UT) as $IT
            }
        }
        impl ZigZag<$UT> for $IT {
            fn zigzag(self) -> $UT {
                ((self << 1) ^ (self >> mem::size_of::<$IT>() * 8 - 1)) as $UT
            }
        }
    }
}
impl_zigzag!(i16, u16);
impl_zigzag!(i32, u32);
impl_zigzag!(i64, u64);
impl_zigzag!(isize, usize);


#[derive(Debug, PartialEq)]
pub enum VarIntResult<I> {
    Complete((I, usize)),
    Incomplete((VarIntIncomplete<I>, usize)),
    Overflow(usize),
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct VarIntIncomplete<I> {
    value: I,
    shift: u32,
}

pub trait VarIntDecode<I> {
    fn decode_varint(available: &[u8]) -> VarIntResult<I>;
}

trait VarIntEncode<I> {
    fn encode_varint(&self) -> VarIntEncoder<I>;
}

#[derive(Debug, PartialEq)]
pub struct VarIntEncoder<I> {
    value: I,
    done: bool,
}


macro_rules! impl_var_int_encoder {
    ($T:ty, $size_hint: expr) => {
        impl Iterator for VarIntEncoder<$T> {
            type Item = u8;

            #[inline]
            fn next(&mut self) -> Option<u8> {
                if self.value >= 0b1000_0000 {
                    let next_byte = (self.value as u8 & 0b0111_1111) | 0b1000_0000;
                    self.value >>= 7;
                    Some(next_byte)
                }
                else if self.done {
                    None
                }
                else {
                    self.done = true;
                    Some(self.value as u8 & 0b0111_1111)
                }
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                (0, Some($size_hint))
            }
        }
    }
}

macro_rules! impl_var_int_encode_usize {
    ($T:ty) => {
        impl VarIntEncode<$T> for $T {
            fn encode_varint(&self) -> VarIntEncoder<$T> {
                VarIntEncoder { value: *self, done: false }
            }
        }
    }
}

macro_rules! impl_var_int_encode_isize {
    ($T:ty, $F: ty) => {
        impl VarIntEncode<$F> for $T {
            fn encode_varint(&self) -> VarIntEncoder<$F> {
                VarIntEncoder { value: (*self).zigzag(), done: false }
            }
        }
    }
}

macro_rules! impl_var_int_decode {
    ($T:ty) => {
        impl VarIntDecode<$T> for $T {
            #[inline]
            fn decode_varint(src: &[u8]) -> VarIntResult<$T> {
                let i = VarIntIncomplete { value: 0 as $T, shift: 0 };
                i.decode(src)
            }
        }
    }
}

macro_rules! impl_var_int_incomplete_usize {
    ($T:ty) => {
        impl VarIntIncomplete<$T>  {
            pub fn decode(&self, src: &[u8]) -> VarIntResult<$T> {
                let mut value = self.value;
                let mut read = 0usize;
                let mut shift = self.shift;
                for b in src.iter() {
                    read += 1;
                    let r = b & 0b01111111;
                    value |= (r as $T) << shift;
                    if b & 0b10000000 == 0 {
                        if shift + 8 - r.leading_zeros() > mem::size_of::<$T>() as u32 * 8 {
                            return VarIntResult::Overflow(read);
                        }
                        return VarIntResult::Complete((value, read));
                    }
                    shift += 7;
                    if shift >= mem::size_of::<$T>() as u32 * 8 {
                        return VarIntResult::Overflow(read);
                    }
                }
                return VarIntResult::Incomplete((VarIntIncomplete { value, shift }, read));
            }
        }
        impl_var_int_decode!($T);
    }
}

macro_rules! impl_var_int_incomplete_isize {
    ($T:ty, $F: ty) => {
        impl VarIntIncomplete<$T>  {
            fn decode(&self, src: &[u8]) -> VarIntResult<$T> {
                let mut value = self.value as $F;
                let mut read = 0usize;
                let mut shift = self.shift;
                for b in src.iter() {
                    read += 1;
                    let r = b & 0b01111111;
                    value |= (r as $F) << shift;
                    if b & 0b10000000 == 0 {
                        println!("{:} + 8 - {:}", shift, r.leading_zeros());
                        if shift + 8 - r.leading_zeros() > mem::size_of::<$T>() as u32 * 8 {
                            return VarIntResult::Overflow(read);
                        }
                        return VarIntResult::Complete((value.zigzag(), read));
                    }
                    shift += 7;
                    if shift >= mem::size_of::<$T>() as u32 * 8 {
                        return VarIntResult::Overflow(read);
                    }
                }
                return VarIntResult::Incomplete((VarIntIncomplete { value: value as $T, shift }, read));
            }
        }
        impl_var_int_decode!($T);
    }
}

impl_var_int_encode_isize!(i16, u16);
impl_var_int_encode_isize!(i32, u32);
impl_var_int_encode_isize!(i64, u64);
impl_var_int_encode_isize!(isize, usize);
impl_var_int_encode_usize!(u16);
impl_var_int_encode_usize!(u32);
impl_var_int_encode_usize!(u64);
impl_var_int_encode_usize!(usize);
impl_var_int_encoder!(u16, 3);
impl_var_int_encoder!(u32, 4);
impl_var_int_encoder!(u64, 10);
impl_var_int_encoder!(usize, 10);
impl_var_int_incomplete_isize!(i16, u16);
impl_var_int_incomplete_isize!(i32, u32);
impl_var_int_incomplete_isize!(i64, u64);
impl_var_int_incomplete_isize!(isize, usize);
impl_var_int_incomplete_usize!(u16);
impl_var_int_incomplete_usize!(u32);
impl_var_int_incomplete_usize!(u64);
impl_var_int_incomplete_usize!(usize);


#[cfg(test)]
mod tests {
    use varint::{VarIntDecode, VarIntResult, VarIntEncode};
    use std::mem;

    #[test]
    fn u64_decode_varint_1() {
        assert_eq!(u64::decode_varint(&[0x1]), VarIntResult::Complete((1, 1)));
        assert_eq!(u64::decode_varint(&[0xBC, 0x41]), VarIntResult::Complete((0x20BC, 2)));
        assert_eq!(u64::decode_varint(&[0xC5, 0x64]), VarIntResult::Complete((0x3245, 2)));
    }

    #[test]
    fn decode_300() {
        assert_eq!(u64::decode_varint(&[0b1010_1100, 0b0000_0010]), VarIntResult::Complete((300, 2)));
        assert_eq!(u32::decode_varint(&[0b1010_1100, 0b0000_0010]), VarIntResult::Complete((300, 2)));
        assert_eq!(u16::decode_varint(&[0b1010_1100, 0b0000_0010]), VarIntResult::Complete((300, 2)));
    }

    #[test]
    fn decode_int_0_2_1() {
        assert_eq!(i64::decode_varint(&[0]), VarIntResult::Complete((0, 1)));
        assert_eq!(i64::decode_varint(&[2]), VarIntResult::Complete((1, 1)));
        assert_eq!(i64::decode_varint(&[1]), VarIntResult::Complete((-1, 1)));
    }


    macro_rules! impl_tests {
        ($T:ty, $overflow_name:ident) => {
            #[test]
            fn $overflow_name() {
                let max_value = &mut <$T>::max_value().encode_varint().collect::< Vec<_>>();
                { *max_value.last_mut().unwrap() += 1; }
                assert_eq!(<$T>::decode_varint(&max_value), VarIntResult::Overflow(max_value.len()));
            }

        }
    }
    macro_rules! impl_tests_usize {
        ($T:ty, $encode_decode_name:ident, $overflow_name:ident) => {
            #[test]
            fn $encode_decode_name() {
                assert_eq!(<$T>::decode_varint(&0.encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((0, 1)));
                let mut n = 0;
                while (n + 1) * 7 < mem::size_of::<$T>() * 8 {
                    let b = 1 << n * 7;
                    n += 1;
                    let e = 1 << n * 7;
                    assert_eq!(<$T>::decode_varint(&b.encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((b, n)));
                    assert_eq!(<$T>::decode_varint(&(e-1).encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((e - 1, n)));
                }
                assert_eq!(<$T>::decode_varint(&<$T>::max_value().encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((<$T>::max_value(), n + 1)));
            }

            impl_tests!($T, $overflow_name);
        }
    }

    macro_rules! impl_tests_isize {
        ($T:ty, $encode_decode_name:ident, $overflow_name:ident) => {
            #[test]
            fn $encode_decode_name() {
                assert_eq!(<$T>::decode_varint(&0.encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((0, 1)));
                let mut n = 0;
                while (n + 1) * 7 < mem::size_of::<$T>() * 8 {
                    let b = 1 << n * 7;
                    n += 1;
                    let pe: $T = 1 << n * 7;
                    let me = pe.wrapping_neg();
                    assert_eq!(<$T>::decode_varint(&b.encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((b, n)));
                    assert_eq!(<$T>::decode_varint(&pe.encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((pe, n + 1)));
                    assert_eq!(<$T>::decode_varint(&me.encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((me, n + 1)));
                }
                assert_eq!(<$T>::decode_varint(&<$T>::max_value().encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((<$T>::max_value(), n + 1)));
            }

            impl_tests!($T, $overflow_name);
        }
    }

    impl_tests_usize!(u64  , encode_decode_u64  , overflow_u64);
    impl_tests_usize!(u32  , encode_decode_u32  , overflow_u32);
    impl_tests_usize!(u16  , encode_decode_u16  , overflow_u16);
    impl_tests_usize!(usize, encode_decode_usize, overflow_usize);
    impl_tests_isize!(i64  , encode_decode_i64  , overflow_i64);
    impl_tests_isize!(i32  , encode_decode_i32  , overflow_i32);
    impl_tests_isize!(i16  , encode_decode_i16  , overflow_i16);
    impl_tests_isize!(isize, encode_decode_isize, overflow_isize);

}

