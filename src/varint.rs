use nom;
use nom::{IResult, Needed};
use std::mem;

#[derive(Copy, Clone, Debug, PartialEq)]
pub struct VarIntIncomplete<I> {
    value: I,
    shift: u32,
}

pub trait VarIntDecode<I> {
    fn decode_varint(i: &[u8]) -> IResult<&[u8], I>;
}

trait VarIntEncode<I> {
    fn encode_varint(&self) -> VarIntEncoder<I>;
}

#[derive(Debug, PartialEq)]
pub struct VarIntEncoder<I> {
    value: I,
    remain: u8,
}

macro_rules! impl_var_int_encoder {
    ($T:ty) => {
        impl Iterator for VarIntEncoder<$T> {
            type Item = u8;

            #[inline]
            fn next(&mut self) -> Option<u8> {
                if self.remain > 1 {
                    self.remain -= 1;
                    let v = self.value >> self.remain * 7;
                    let next_byte = (v as u8 & 0b0111_1111) | 0b1000_0000;
                    Some(next_byte)
                } else if self.remain == 1 {
                    self.remain -= 1;
                    Some(self.value as u8 & 0b0111_1111)
                } else {
                    None
                }
            }

            #[inline]
            fn size_hint(&self) -> (usize, Option<usize>) {
                (0, Some(self.remain as usize))
            }
        }
    };
}

macro_rules! impl_var_int_encode_usize {
    ($T:ty) => {
        impl VarIntEncode<$T> for $T {
            fn encode_varint(&self) -> VarIntEncoder<$T> {
                let r = if *self > 0 {
                    (6 + mem::size_of::<$T>() as u8 * 8 - self.leading_zeros() as u8) / 7
                } else {
                    1
                };
                VarIntEncoder {
                    value: *self,
                    remain: r,
                }
            }
        }
    };
}

macro_rules! impl_var_int_decode {
    ($T:ty) => {
        impl VarIntDecode<$T> for $T {
            fn decode_varint(i: &[u8]) -> IResult<&[u8], $T> {
                let mut value = 0 as $T;
                let mut read = 0usize;
                for b in i.iter() {
                    read += 1;
                    let r = b & 0b01111111;
                    value |= r as $T;
                    if b & 0b10000000 == 0 {
                        return IResult::Done(&i[read..], value);
                    }
                    if value > <$T>::max_value() >> 7 {
                        return IResult::Error(nom::ErrorKind::Custom(1));
                    }
                    value <<= 7;
                }
                IResult::Incomplete(Needed::Unknown)
            }
        }
    };
}

impl_var_int_encode_usize!(u16);
impl_var_int_encode_usize!(u32);
impl_var_int_encode_usize!(u64);
impl_var_int_encode_usize!(usize);
impl_var_int_encoder!(u16);
impl_var_int_encoder!(u32);
impl_var_int_encoder!(u64);
impl_var_int_encoder!(usize);
impl_var_int_decode!(u16);
impl_var_int_decode!(u32);
impl_var_int_decode!(u64);
impl_var_int_decode!(usize);

#[cfg(test)]
mod tests {
    use nom;
    use nom::IResult;
    use varint::{VarIntDecode, VarIntEncode};

    macro_rules! impl_tests {
        ($T:ty, $overflow_name:ident) => {
            #[test]
            fn $overflow_name() {
                let max_value = &mut <$T>::max_value().encode_varint().collect::<Vec<_>>();
                {
                    *max_value.last_mut().unwrap() += 1;
                }
                assert_eq!(
                    <$T>::decode_varint(&max_value),
                    IResult::Error(nom::ErrorKind::Custom(1))
                );
            }
        };
    }
    macro_rules! impl_tests_usize {
        ($T:ty, $encode_decode_name:ident, $overflow_name:ident) => {
            #[test]
            fn $encode_decode_name() {
                assert_eq!(
                    <$T>::decode_varint(&<$T>::min_value().encode_varint().collect::<Vec<_>>()),
                    IResult::Done(&b""[..], 0)
                );
                assert_eq!(
                    <$T>::decode_varint(&(1 as $T).encode_varint().collect::<Vec<_>>()),
                    IResult::Done(&b""[..], 1)
                );
                /*let mut n = 0;
                while (n + 1) * 7 < mem::size_of::<$T>() * 8 {
                    let b = 1 << n * 7;
                    n += 1;
                    let e = 1 << n * 7;
                    //assert_eq!(<$T>::decode_varint(&b.encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((b, n)));
                    //assert_eq!(<$T>::decode_varint(&(e-1).encode_varint().collect::<Vec<_>>()), VarIntResult::Complete((e - 1, n)));
                }*/
                assert_eq!(
                    <$T>::decode_varint(&<$T>::max_value().encode_varint().collect::<Vec<_>>()),
                    IResult::Done(&b""[..], <$T>::max_value())
                );
            }

            impl_tests!($T, $overflow_name);
        };
    }

    impl_tests_usize!(u64, encode_decode_u64, overflow_u64);
    impl_tests_usize!(u32, encode_decode_u32, overflow_u32);
    impl_tests_usize!(u16, encode_decode_u16, overflow_u16);
    impl_tests_usize!(usize, encode_decode_usize, overflow_usize);
}
