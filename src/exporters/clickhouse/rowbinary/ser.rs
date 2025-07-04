use crate::exporters::clickhouse::ch_error::{Error, Result};
use bytes::BufMut;
use serde::ser::SerializeMap;
use serde::{
    Serialize,
    ser::{Impossible, SerializeSeq, SerializeStruct, SerializeTuple, Serializer},
};

/// Serializes `value` using the RowBinary format and writes to `buffer`.
pub(crate) fn serialize_into(buffer: impl BufMut, value: &impl Serialize) -> Result<()> {
    let mut serializer = RowBinarySerializer { buffer };
    value.serialize(&mut serializer)?;
    Ok(())
}

/// A serializer for the RowBinary format.
///
/// See https://clickhouse.com/docs/en/interfaces/formats#rowbinary for details.
struct RowBinarySerializer<B> {
    buffer: B,
}

macro_rules! impl_num {
    ($ty:ty, $ser_method:ident, $writer_method:ident) => {
        #[inline]
        fn $ser_method(self, v: $ty) -> Result<()> {
            self.buffer.$writer_method(v);
            Ok(())
        }
    };
}

impl<B: BufMut> Serializer for &'_ mut RowBinarySerializer<B> {
    type Error = Error;
    type Ok = ();
    type SerializeMap = Self;
    type SerializeSeq = Self;
    type SerializeStruct = Self;
    type SerializeStructVariant = Impossible<(), Error>;
    type SerializeTuple = Self;
    type SerializeTupleStruct = Impossible<(), Error>;
    type SerializeTupleVariant = Impossible<(), Error>;

    impl_num!(i8, serialize_i8, put_i8);

    impl_num!(i16, serialize_i16, put_i16_le);

    impl_num!(i32, serialize_i32, put_i32_le);

    impl_num!(i64, serialize_i64, put_i64_le);

    impl_num!(i128, serialize_i128, put_i128_le);

    impl_num!(u8, serialize_u8, put_u8);

    impl_num!(u16, serialize_u16, put_u16_le);

    impl_num!(u32, serialize_u32, put_u32_le);

    impl_num!(u64, serialize_u64, put_u64_le);

    impl_num!(u128, serialize_u128, put_u128_le);

    impl_num!(f32, serialize_f32, put_f32_le);

    impl_num!(f64, serialize_f64, put_f64_le);

    #[inline]
    fn serialize_bool(self, v: bool) -> Result<()> {
        self.buffer.put_u8(v as _);
        Ok(())
    }

    #[inline]
    fn serialize_char(self, _v: char) -> Result<()> {
        panic!("character types are unsupported: `char`");
    }

    #[inline]
    fn serialize_str(self, v: &str) -> Result<()> {
        put_unsigned_leb128(&mut self.buffer, v.len() as u64);
        self.buffer.put_slice(v.as_bytes());
        Ok(())
    }

    #[inline]
    fn serialize_bytes(self, v: &[u8]) -> Result<()> {
        put_unsigned_leb128(&mut self.buffer, v.len() as u64);
        self.buffer.put_slice(v);
        Ok(())
    }

    #[inline]
    fn serialize_none(self) -> Result<()> {
        self.buffer.put_u8(1);
        Ok(())
    }

    #[inline]
    fn serialize_some<T: Serialize + ?Sized>(self, value: &T) -> Result<()> {
        self.buffer.put_u8(0);
        value.serialize(self)
    }

    #[inline]
    fn serialize_unit(self) -> Result<()> {
        panic!("unit types are unsupported: `()`");
    }

    #[inline]
    fn serialize_unit_struct(self, name: &'static str) -> Result<()> {
        panic!("unit types are unsupported: `{name}`");
    }

    #[inline]
    fn serialize_unit_variant(
        self,
        name: &'static str,
        _variant_index: u32,
        variant: &'static str,
    ) -> Result<()> {
        panic!("unit variant types are unsupported: `{name}::{variant}`");
    }

    #[inline]
    fn serialize_newtype_struct<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<()> {
        value.serialize(self)
    }

    #[inline]
    fn serialize_newtype_variant<T: Serialize + ?Sized>(
        self,
        _name: &'static str,
        variant_index: u32,
        _variant: &'static str,
        value: &T,
    ) -> Result<()> {
        // TODO:
        //  - Now this code implicitly allows using enums at the top level.
        //    However, instead of a more descriptive panic, it ends with a "not enough data." error.
        //  - Also, it produces an unclear message for a forgotten `serde_repr` (Enum8 and Enum16).
        //  See https://github.com/ClickHouse/clickhouse-rs/pull/170#discussion_r1848549636

        // Max number of types in the Variant data type is 255
        // See also: https://github.com/ClickHouse/ClickHouse/issues/54864
        if variant_index > 255 {
            return Err(Error::VariantDiscriminatorIsOutOfBound(
                variant_index as usize,
            ));
        }
        self.buffer.put_u8(variant_index as u8);
        value.serialize(self)
    }

    #[inline]
    fn serialize_seq(self, len: Option<usize>) -> Result<Self::SerializeSeq> {
        let len = len.ok_or(Error::SequenceMustHaveLength)?;
        put_unsigned_leb128(&mut self.buffer, len as u64);
        Ok(self)
    }

    #[inline]
    fn serialize_tuple(self, _len: usize) -> Result<Self::SerializeTuple> {
        Ok(self)
    }

    #[inline]
    fn serialize_tuple_struct(
        self,
        name: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleStruct> {
        panic!("tuple struct types are unsupported: `{name}`");
    }

    #[inline]
    fn serialize_tuple_variant(
        self,
        name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeTupleVariant> {
        panic!("tuple variant types are unsupported: `{name}::{variant}`");
    }

    #[inline]
    fn serialize_map(self, _len: Option<usize>) -> Result<Self::SerializeMap> {
        //panic!("maps are unsupported, use `Vec<(A, B)>` instead");
        Ok(self)
    }

    #[inline]
    fn serialize_struct(self, _name: &'static str, _len: usize) -> Result<Self::SerializeStruct> {
        Ok(self)
    }

    #[inline]
    fn serialize_struct_variant(
        self,
        name: &'static str,
        _variant_index: u32,
        variant: &'static str,
        _len: usize,
    ) -> Result<Self::SerializeStructVariant> {
        panic!("struct variant types are unsupported: `{name}::{variant}`");
    }

    #[inline]
    fn is_human_readable(&self) -> bool {
        false
    }
}

impl<B: BufMut> SerializeStruct for &mut RowBinarySerializer<B> {
    type Error = Error;
    type Ok = ();

    #[inline]
    fn serialize_field<T: Serialize + ?Sized>(&mut self, _: &'static str, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        Ok(())
    }
}

// We don't fully support serializing maps, but we must implement this to support
// flattened serde structs. We skip any serializing of the size or keys, we just rely
// on the value serializers.
impl<B: BufMut> SerializeMap for &mut RowBinarySerializer<B> {
    type Ok = ();
    type Error = Error;

    fn serialize_key<T>(&mut self, _key: &T) -> std::result::Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        Ok(())
    }

    fn serialize_value<T>(&mut self, value: &T) -> std::result::Result<(), Self::Error>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    fn end(self) -> std::result::Result<Self::Ok, Self::Error> {
        Ok(())
    }
}

impl<B: BufMut> SerializeSeq for &'_ mut RowBinarySerializer<B> {
    type Error = Error;
    type Ok = ();

    fn serialize_element<T: Serialize + ?Sized>(&mut self, value: &T) -> Result<()> {
        value.serialize(&mut **self)
    }

    fn end(self) -> Result<()> {
        Ok(())
    }
}

impl<B: BufMut> SerializeTuple for &'_ mut RowBinarySerializer<B> {
    type Error = Error;
    type Ok = ();

    #[inline]
    fn serialize_element<T>(&mut self, value: &T) -> Result<()>
    where
        T: ?Sized + Serialize,
    {
        value.serialize(&mut **self)
    }

    #[inline]
    fn end(self) -> Result<()> {
        Ok(())
    }
}

fn put_unsigned_leb128(mut buffer: impl BufMut, mut value: u64) {
    while {
        let mut byte = value as u8 & 0x7f;
        value >>= 7;

        if value != 0 {
            byte |= 0x80;
        }

        buffer.put_u8(byte);

        value != 0
    } {}
}

#[test]
fn it_serializes_unsigned_leb128() {
    let mut vec = Vec::new();

    put_unsigned_leb128(&mut vec, 624_485);

    assert_eq!(vec, [0xe5, 0x8e, 0x26]);
}
