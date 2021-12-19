use crate::error::Error;

use super::tlv_common::*;

use byteorder::{ByteOrder, LittleEndian};
use log::{error, info};
use std::fmt;

pub struct TLVList<'a> {
    buf: &'a [u8],
    len: usize,
}

impl<'a> TLVList<'a> {
    pub fn new(buf: &'a [u8], len: usize) -> TLVList<'a> {
        TLVList { buf, len }
    }
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Pointer<'a> {
    buf: &'a [u8],
    current: usize,
    left: usize,
}

#[derive(Debug, Copy, Clone, PartialEq)]
pub enum ElementType<'a> {
    S8(i8),
    S16(i16),
    S32(i32),
    S64(i64),
    U8(u8),
    U16(u16),
    U32(u32),
    U64(u64),
    False,
    True,
    F32(f32),
    F64(f64),
    Utf8l(&'a [u8]),
    Utf16l,
    Utf32l,
    Utf64l,
    Str8l(&'a [u8]),
    Str16l,
    Str32l,
    Str64l,
    Null,
    Struct(Pointer<'a>),
    Array(Pointer<'a>),
    List(Pointer<'a>),
    EndCnt,
    Last,
}

const MAX_VALUE_INDEX: usize = 25;

// This is a function that takes a TLVListIterator and returns the tag type
type ExtractTag = for<'a> fn(&TLVListIterator<'a>) -> TagType;
static TAG_EXTRACTOR: [ExtractTag; 8] = [
    // Anonymous 0
    |_t| TagType::Anonymous,
    // Context 1
    |t| TagType::Context(t.buf[t.current]),
    // CommonPrf16 2
    |t| TagType::CommonPrf16(LittleEndian::read_u16(&t.buf[t.current..])),
    // CommonPrf32 3
    |t| TagType::CommonPrf32(LittleEndian::read_u32(&t.buf[t.current..])),
    // ImplPrf16 4
    |t| TagType::ImplPrf16(LittleEndian::read_u16(&t.buf[t.current..])),
    // ImplPrf32 5
    |t| TagType::ImplPrf32(LittleEndian::read_u32(&t.buf[t.current..])),
    // FullQual48 6
    |t| TagType::FullQual48(LittleEndian::read_u48(&t.buf[t.current..]) as u64),
    // FullQual64 7
    |t| TagType::FullQual64(LittleEndian::read_u64(&t.buf[t.current..])),
];

// This is a function that takes a TLVListIterator and returns the element type
// Some elements (like strings), also consume additional size, than that mentioned
// if this is the case, the additional size is returned
type ExtractValue = for<'a> fn(&TLVListIterator<'a>) -> (usize, ElementType<'a>);

static VALUE_EXTRACTOR: [ExtractValue; MAX_VALUE_INDEX] = [
    // S8   0
    { |t| (0, ElementType::S8(t.buf[t.current] as i8)) },
    // S16  1
    {
        |t| {
            (
                0,
                ElementType::S16(LittleEndian::read_i16(&t.buf[t.current..])),
            )
        }
    },
    // S32  2
    {
        |t| {
            (
                0,
                ElementType::S32(LittleEndian::read_i32(&t.buf[t.current..])),
            )
        }
    },
    // S64  3
    {
        |t| {
            (
                0,
                ElementType::S64(LittleEndian::read_i64(&t.buf[t.current..])),
            )
        }
    },
    // U8   4
    { |t| (0, ElementType::U8(t.buf[t.current])) },
    // U16  5
    {
        |t| {
            (
                0,
                ElementType::U16(LittleEndian::read_u16(&t.buf[t.current..])),
            )
        }
    },
    // U32  6
    {
        |t| {
            (
                0,
                ElementType::U32(LittleEndian::read_u32(&t.buf[t.current..])),
            )
        }
    },
    // U64  7
    {
        |t| {
            (
                0,
                ElementType::U64(LittleEndian::read_u64(&t.buf[t.current..])),
            )
        }
    },
    // False 8
    { |_t| (0, ElementType::False) },
    // True 9
    { |_t| (0, ElementType::True) },
    // F32  10
    { |_t| (0, ElementType::Last) },
    // F64  11
    { |_t| (0, ElementType::Last) },
    // Utf8l 12
    {
        |t| {
            // The current byte is the string size
            let size: usize = t.buf[t.current] as usize;
            // We'll consume the current byte (len) + the entire string
            if size + 1 > t.left {
                // Return Error
                return (size, ElementType::Last);
            }
            (
                // return the additional size only
                size,
                ElementType::Utf8l(&t.buf[(t.current + 1)..(t.current + 1 + size)]),
            )
        }
    },
    // Utf16l  13
    { |_t| (0, ElementType::Last) },
    // Utf32l 14
    { |_t| (0, ElementType::Last) },
    // Utf64l 15
    { |_t| (0, ElementType::Last) },
    // Str8l 16
    {
        |t| {
            // The current byte is the string size
            let size: usize = t.buf[t.current] as usize;
            // We'll consume the current byte (len) + the entire string
            if size + 1 > t.left {
                // Return Error
                return (size, ElementType::Last);
            }
            (
                // return the additional size only
                size,
                ElementType::Str8l(&t.buf[(t.current + 1)..(t.current + 1 + size)]),
            )
        }
    },
    // Str16l 17
    { |_t| (0, ElementType::Last) },
    // Str32l 18
    { |_t| (0, ElementType::Last) },
    // Str64l 19
    { |_t| (0, ElementType::Last) },
    // Null  20
    { |_t| (0, ElementType::Null) },
    // Struct 21
    {
        |t| {
            (
                0,
                ElementType::Struct(Pointer {
                    buf: t.buf,
                    current: t.current,
                    left: t.left,
                }),
            )
        }
    },
    // Array  22
    {
        |t| {
            (
                0,
                ElementType::Array(Pointer {
                    buf: t.buf,
                    current: t.current,
                    left: t.left,
                }),
            )
        }
    },
    // List  23
    {
        |t| {
            (
                0,
                ElementType::List(Pointer {
                    buf: t.buf,
                    current: t.current,
                    left: t.left,
                }),
            )
        }
    },
    // EndCnt  24
    { |_t| (0, ElementType::EndCnt) },
];

// The array indices here correspond to the numeric value of the Element Type as defined in the Matter Spec
static VALUE_SIZE_MAP: [usize; MAX_VALUE_INDEX] = [
    1, // S8   0
    2, // S16  1
    4, // S32  2
    8, // S64  3
    1, // U8   4
    2, // U16  5
    4, // U32  6
    8, // U64  7
    0, // False 8
    0, // True 9
    4, // F32  10
    8, // F64  11
    1, // Utf8l 12
    2, // Utf16l  13
    4, // Utf32l 14
    8, // Utf64l 15
    1, // Str8l 16
    2, // Str16l 17
    4, // Str32l 18
    8, // Str64l 19
    0, // Null  20
    0, // Struct 21
    0, // Array  22
    0, // List  23
    0, // EndCnt  24
];

#[derive(Debug, Copy, Clone, PartialEq)]
pub struct TLVElement<'a> {
    tag_type: TagType,
    element_type: ElementType<'a>,
}

impl<'a> TLVElement<'a> {
    pub fn into_iter(&self) -> Option<TLVContainerIterator<'a>> {
        let ptr = match self.element_type {
            ElementType::Struct(a) | ElementType::Array(a) | ElementType::List(a) => a,
            _ => return None,
        };
        let list_iter = TLVListIterator {
            buf: ptr.buf,
            current: ptr.current,
            left: ptr.left,
        };
        Some(TLVContainerIterator {
            list_iter,
            prev_container: false,
            iterator_consumed: false,
        })
    }

    pub fn get_u8(&self) -> Result<u8, Error> {
        match self.element_type {
            ElementType::U8(a) => Ok(a),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn get_u16(&self) -> Result<u16, Error> {
        match self.element_type {
            ElementType::U16(a) => Ok(a),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn get_u32(&self) -> Result<u32, Error> {
        match self.element_type {
            ElementType::U32(a) => Ok(a),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn get_u64(&self) -> Result<u64, Error> {
        match self.element_type {
            ElementType::U64(a) => Ok(a),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn get_slice(&self) -> Result<&'a [u8], Error> {
        match self.element_type {
            ElementType::Str8l(s) | ElementType::Utf8l(s) => Ok(s),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn get_bool(&self) -> Result<bool, Error> {
        match self.element_type {
            ElementType::False => Ok(false),
            ElementType::True => Ok(true),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn confirm_struct(&self) -> Result<TLVElement<'a>, Error> {
        match self.element_type {
            ElementType::Struct(_) => Ok(*self),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn confirm_array(&self) -> Result<TLVElement<'a>, Error> {
        match self.element_type {
            ElementType::Array(_) => Ok(*self),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn confirm_list(&self) -> Result<TLVElement<'a>, Error> {
        match self.element_type {
            ElementType::List(_) => Ok(*self),
            _ => Err(Error::TLVTypeMismatch),
        }
    }

    pub fn find_tag(&self, tag: u32) -> Result<TLVElement<'a>, Error> {
        let mut iter = self.into_iter().ok_or(Error::TLVTypeMismatch)?;
        let match_tag: TagType = TagType::Context(tag as u8);
        loop {
            match iter.next() {
                Some(a) => {
                    if match_tag == a.tag_type {
                        return Ok(a);
                    }
                }
                None => return Err(Error::NoTagFound),
            }
        }
    }

    pub fn get_tag(&self) -> TagType {
        self.tag_type
    }
}

impl<'a> fmt::Display for TLVElement<'a> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.tag_type {
            TagType::Anonymous => (),
            TagType::Context(tag) => write!(f, "{}:", tag)?,
            _ => write!(f, "Other Context Tag")?,
        }
        match self.element_type {
            ElementType::Struct(_) => write!(f, "{{"),
            ElementType::Array(_) => write!(f, "["),
            ElementType::List(_) => write!(f, "["),
            ElementType::EndCnt => write!(f, ">"),
            ElementType::True => write!(f, "True"),
            ElementType::False => write!(f, "False"),
            ElementType::Str8l(a) | ElementType::Utf8l(a) => {
                if let Ok(s) = std::str::from_utf8(a) {
                    write!(f, "len[{}]\"{}\"", s.len(), s)
                } else {
                    write!(f, "len[{}]{:x?}", a.len(), a)
                }
            }
            _ => write!(f, "{:?}", self.element_type),
        }
    }
}

// This is a TLV List iterator, it only iterates over the individual TLVs in a TLV list
#[derive(Copy, Clone, Debug, PartialEq)]
pub struct TLVListIterator<'a> {
    buf: &'a [u8],
    current: usize,
    left: usize,
}

impl<'a> TLVListIterator<'a> {
    fn advance(&mut self, len: usize) {
        self.current += len;
        self.left -= len;
    }

    // Caller should ensure they are reading the _right_ tag at the _right_ place
    fn read_this_tag(&mut self, tag_type: u8) -> Option<TagType> {
        if tag_type as usize >= MAX_TAG_INDEX {
            return None;
        }
        let tag_size = TAG_SIZE_MAP[tag_type as usize];
        if tag_size > self.left {
            return None;
        }
        let tag = (TAG_EXTRACTOR[tag_type as usize])(self);
        self.advance(tag_size);
        Some(tag)
    }

    fn read_this_value(&mut self, element_type: u8) -> Option<ElementType<'a>> {
        if element_type as usize >= MAX_VALUE_INDEX {
            return None;
        }
        let mut size = VALUE_SIZE_MAP[element_type as usize];
        if size > self.left {
            error!("Invalid value found: {}", element_type);
            return None;
        }

        let (extra_size, element) = (VALUE_EXTRACTOR[element_type as usize])(self);
        if element != ElementType::Last {
            size += extra_size;
            self.advance(size);
            Some(element)
        } else {
            None
        }
    }
}

impl<'a> TLVListIterator<'a> {
    /* Code for going to the next Element */
    pub fn next(&mut self) -> Option<TLVElement<'a>> {
        if self.left < 1 {
            return None;
        }
        /* Read Control */
        let control = self.buf[self.current];
        let tag_type = (control & TAG_MASK) >> TAG_SHIFT_BITS;
        let element_type = control & TYPE_MASK;
        self.advance(1);

        /* Consume Tag */
        let tag_type = self.read_this_tag(tag_type)?;

        /* Consume Value */
        let element_type = self.read_this_value(element_type)?;

        Some(TLVElement {
            tag_type,
            element_type,
        })
    }
}

impl<'a> TLVList<'a> {
    pub fn into_iter(&self) -> TLVListIterator<'a> {
        TLVListIterator {
            current: 0,
            left: self.len,
            buf: self.buf,
        }
    }
}

fn is_container(element_type: ElementType) -> bool {
    matches!(
        element_type,
        ElementType::Struct(_) | ElementType::Array(_) | ElementType::List(_)
    )
}

// This is a Container iterator, it iterates over containers in a TLV list
#[derive(Debug, PartialEq)]
pub struct TLVContainerIterator<'a> {
    list_iter: TLVListIterator<'a>,
    prev_container: bool,
    iterator_consumed: bool,
}

impl<'a> TLVContainerIterator<'a> {
    fn skip_to_end_of_container(&mut self) -> Option<TLVElement<'a>> {
        let mut nest_level = 0;
        while let Some(element) = self.list_iter.next() {
            // We know we are already in a container, we have to keep looking for end-of-container
            //            println!("Skip: element: {:x?} nest_level: {}", element, nest_level);
            match element.element_type {
                ElementType::EndCnt => {
                    if nest_level == 0 {
                        // Return the element following this element
                        //                        println!("Returning");
                        // The final next() may be the end of the top-level container itself, if so, we must return None
                        let last_elem = self.list_iter.next()?;
                        match last_elem.element_type {
                            ElementType::EndCnt => {
                                self.iterator_consumed = true;
                                return None;
                            }
                            _ => return Some(last_elem),
                        }
                    } else {
                        nest_level -= 1;
                    }
                }
                _ => {
                    if is_container(element.element_type) {
                        nest_level += 1;
                    }
                }
            }
        }
        None
    }
}

impl<'a> TLVContainerIterator<'a> {
    /* Code for going to the next Element */
    pub fn next(&mut self) -> Option<TLVElement<'a>> {
        // This iterator may be consumed, but the underlying might not. This protects it from such occurrences
        if self.iterator_consumed {
            return None;
        }
        let element: TLVElement = if self.prev_container {
            //            println!("Calling skip to end of container");
            self.skip_to_end_of_container()?
        } else {
            self.list_iter.next()?
        };
        //        println!("Found element: {:x?}", element);
        /* If we found end of container, that means our own container is over */
        if element.element_type == ElementType::EndCnt {
            self.iterator_consumed = true;
            return None;
        }

        if is_container(element.element_type) {
            self.prev_container = true;
        } else {
            self.prev_container = false;
        }
        Some(element)
    }
}

pub fn get_root_node_struct(b: &[u8]) -> Result<TLVElement, Error> {
    return TLVList::new(b, b.len())
        .into_iter()
        .next()
        .ok_or(Error::InvalidData)?
        .confirm_struct();
}

pub fn get_root_node_list(b: &[u8]) -> Result<TLVElement, Error> {
    return TLVList::new(b, b.len())
        .into_iter()
        .next()
        .ok_or(Error::InvalidData)?
        .confirm_list();
}

pub fn print_tlv_list(b: &[u8]) {
    let tlvlist = TLVList::new(b, b.len());

    info!("TLV list:");
    let mut iter = tlvlist.into_iter();
    while let Some(a) = iter.next() {
        info!("{}", a)
    }
    info!("---------");
}

#[cfg(test)]
mod tests {
    use crate::tlv::*;

    #[test]
    fn test_short_length_tag() {
        // The 0x36 is an array with a tag, but we leave out the tag field
        let b = [0x15, 0x36];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(tlv_iter.next(), None);
    }

    #[test]
    fn test_invalid_value_type() {
        // The 0x24 is a a tagged integer, here we leave out the integer value
        let b = [0x15, 0x1f, 0x0];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(tlv_iter.next(), None);
    }

    #[test]
    fn test_short_length_value_immediate() {
        // The 0x24 is a a tagged integer, here we leave out the integer value
        let b = [0x15, 0x24, 0x0];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(tlv_iter.next(), None);
    }

    #[test]
    fn test_short_length_value_string() {
        // This is a tagged string, with tag 0 and length 0xb, but we only have 4 bytes in the string
        let b = [0x15, 0x30, 0x00, 0x0b, 0x73, 0x6d, 0x61, 0x72];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(tlv_iter.next(), None);
    }

    #[test]
    fn test_valid_tag() {
        // The 0x36 is an array with a tag, here tag is 0
        let b = [0x15, 0x36, 0x0];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(
            tlv_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(0),
                element_type: ElementType::Array(Pointer {
                    buf: &[21, 54, 0],
                    current: 3,
                    left: 0
                }),
            })
        );
    }

    #[test]
    fn test_valid_value_immediate() {
        // The 0x24 is a a tagged integer, here the integer is 2
        let b = [0x15, 0x24, 0x1, 0x2];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(
            tlv_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(1),
                element_type: ElementType::U8(2),
            })
        );
    }

    #[test]
    fn test_valid_value_string() {
        // This is a tagged string, with tag 0 and length 4, and we have 4 bytes in the string
        let b = [0x15, 0x30, 0x5, 0x04, 0x73, 0x6d, 0x61, 0x72];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(
            tlv_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(5),
                element_type: ElementType::Str8l(&[0x73, 0x6d, 0x61, 0x72]),
            })
        );
    }

    #[test]
    fn test_no_iterator_for_int() {
        // The 0x24 is a a tagged integer, here the integer is 2
        let b = [0x15, 0x24, 0x1, 0x2];
        let tlvlist = TLVList::new(&b, b.len());
        let mut tlv_iter = tlvlist.into_iter();
        // Skip the 0x15
        tlv_iter.next();
        assert_eq!(tlv_iter.next().unwrap().into_iter(), None);
    }

    #[test]
    fn test_struct_iteration_with_mix_values() {
        // This is a struct with 3 valid values
        let b = [
            0x15, 0x24, 0x0, 0x2, 0x26, 0x2, 0x4e, 0x10, 0x02, 0x00, 0x30, 0x3, 0x04, 0x73, 0x6d,
            0x61, 0x72,
        ];
        let mut root_iter = get_root_node_struct(&b).unwrap().into_iter().unwrap();
        assert_eq!(
            root_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(0),
                element_type: ElementType::U8(2),
            })
        );
        assert_eq!(
            root_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(2),
                element_type: ElementType::U32(135246),
            })
        );
        assert_eq!(
            root_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(3),
                element_type: ElementType::Str8l(&[0x73, 0x6d, 0x61, 0x72]),
            })
        );
    }

    #[test]
    fn test_struct_find_element_mix_values() {
        // This is a struct with 3 valid values
        let b = [
            0x15, 0x30, 0x3, 0x04, 0x73, 0x6d, 0x61, 0x72, 0x24, 0x0, 0x2, 0x26, 0x2, 0x4e, 0x10,
            0x02, 0x00,
        ];
        let root = get_root_node_struct(&b).unwrap();

        assert_eq!(
            root.find_tag(0).unwrap(),
            TLVElement {
                tag_type: TagType::Context(0),
                element_type: ElementType::U8(2),
            }
        );
        assert_eq!(
            root.find_tag(2).unwrap(),
            TLVElement {
                tag_type: TagType::Context(2),
                element_type: ElementType::U32(135246),
            }
        );
        assert_eq!(
            root.find_tag(3).unwrap(),
            TLVElement {
                tag_type: TagType::Context(3),
                element_type: ElementType::Str8l(&[0x73, 0x6d, 0x61, 0x72]),
            }
        );
    }

    #[test]
    fn test_list_iteration_with_mix_values() {
        // This is a list with 3 valid values
        let b = [
            0x17, 0x24, 0x0, 0x2, 0x26, 0x2, 0x4e, 0x10, 0x02, 0x00, 0x30, 0x3, 0x04, 0x73, 0x6d,
            0x61, 0x72,
        ];
        let mut root_iter = get_root_node_list(&b).unwrap().into_iter().unwrap();
        assert_eq!(
            root_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(0),
                element_type: ElementType::U8(2),
            })
        );
        assert_eq!(
            root_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(2),
                element_type: ElementType::U32(135246),
            })
        );
        assert_eq!(
            root_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(3),
                element_type: ElementType::Str8l(&[0x73, 0x6d, 0x61, 0x72]),
            })
        );
    }

    #[test]
    fn test_complex_structure_invoke_cmd() {
        // This is what we typically get in an invoke command
        let b = [
            0x15, 0x36, 0x0, 0x15, 0x37, 0x0, 0x24, 0x0, 0x2, 0x24, 0x2, 0x6, 0x24, 0x3, 0x1, 0x18,
            0x35, 0x1, 0x18, 0x18, 0x18, 0x18,
        ];

        let root = get_root_node_struct(&b).unwrap();

        let mut cmd_list_iter = root
            .find_tag(0)
            .unwrap()
            .confirm_array()
            .unwrap()
            .into_iter()
            .unwrap();
        println!("Command list iterator: {:?}", cmd_list_iter);

        // This is an array of CommandDataIB, but we'll only use the first element
        let cmd_data_ib = cmd_list_iter.next().unwrap();

        let cmd_path = cmd_data_ib.find_tag(0).unwrap().confirm_list().unwrap();
        assert_eq!(
            cmd_path.find_tag(0).unwrap(),
            TLVElement {
                tag_type: TagType::Context(0),
                element_type: ElementType::U8(2),
            }
        );
        assert_eq!(
            cmd_path.find_tag(2).unwrap(),
            TLVElement {
                tag_type: TagType::Context(2),
                element_type: ElementType::U8(6),
            }
        );
        assert_eq!(
            cmd_path.find_tag(3).unwrap(),
            TLVElement {
                tag_type: TagType::Context(3),
                element_type: ElementType::U8(1),
            }
        );
        assert_eq!(cmd_path.find_tag(1), Err(Error::NoTagFound));

        // This is the variable of the invoke command
        assert_eq!(
            cmd_data_ib.find_tag(1).unwrap().into_iter().unwrap().next(),
            None
        );
    }

    #[test]
    fn test_read_past_end_of_container() {
        let b = [0x15, 0x35, 0x0, 0x24, 0x1, 0x2, 0x18, 0x24, 0x0, 0x2, 0x18];

        let mut sub_root_iter = get_root_node_struct(&b)
            .unwrap()
            .find_tag(0)
            .unwrap()
            .into_iter()
            .unwrap();
        assert_eq!(
            sub_root_iter.next(),
            Some(TLVElement {
                tag_type: TagType::Context(1),
                element_type: ElementType::U8(2),
            })
        );
        assert_eq!(sub_root_iter.next(), None);
        // Call next, even after the first next returns None
        assert_eq!(sub_root_iter.next(), None);
        assert_eq!(sub_root_iter.next(), None);
    }

    #[test]
    fn test_basic_list_iterator() {
        // This is the input we have
        let b = [
            0x15, 0x36, 0x0, 0x15, 0x37, 0x0, 0x24, 0x0, 0x2, 0x24, 0x2, 0x6, 0x24, 0x3, 0x1, 0x18,
            0x35, 0x1, 0x18, 0x18, 0x18, 0x18,
        ];

        let dummy_pointer = Pointer {
            buf: &b,
            current: 1,
            left: 21,
        };
        // These are the decoded elements that we expect from this input
        let verify_matrix: [(TagType, ElementType); 13] = [
            (TagType::Anonymous, ElementType::Struct(dummy_pointer)),
            (TagType::Context(0), ElementType::Array(dummy_pointer)),
            (TagType::Anonymous, ElementType::Struct(dummy_pointer)),
            (TagType::Context(0), ElementType::List(dummy_pointer)),
            (TagType::Context(0), ElementType::U8(2)),
            (TagType::Context(2), ElementType::U8(6)),
            (TagType::Context(3), ElementType::U8(1)),
            (TagType::Anonymous, ElementType::EndCnt),
            (TagType::Context(1), ElementType::Struct(dummy_pointer)),
            (TagType::Anonymous, ElementType::EndCnt),
            (TagType::Anonymous, ElementType::EndCnt),
            (TagType::Anonymous, ElementType::EndCnt),
            (TagType::Anonymous, ElementType::EndCnt),
        ];

        let mut list_iter = TLVList::new(&b, b.len()).into_iter();
        let mut index = 0;
        loop {
            let element = list_iter.next();
            match element {
                None => break,
                Some(a) => {
                    assert_eq!(a.tag_type, verify_matrix[index].0);
                    assert_eq!(
                        std::mem::discriminant(&a.element_type),
                        std::mem::discriminant(&verify_matrix[index].1)
                    );
                }
            }
            index += 1;
        }
        // After the end, purposefully try a few more next
        assert_eq!(list_iter.next(), None);
        assert_eq!(list_iter.next(), None);
    }
}
