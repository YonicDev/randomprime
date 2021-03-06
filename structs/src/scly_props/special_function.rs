use auto_struct_macros::auto_struct;

use reader_writer::CStr;
use reader_writer::typenum::*;
use reader_writer::generic_array::GenericArray;
use crate::SclyPropertyData;

#[auto_struct(Readable, Writable)]
#[derive(Debug, Clone)]
pub struct SpecialFunction<'r>
{
    #[auto_struct(expect = 15)]
    prop_count: u32,

    pub name: CStr<'r>,

    pub position: GenericArray<f32, U3>,
    pub rotation: GenericArray<f32, U3>,

    pub type_: u32,

    pub unknown0: CStr<'r>,
    pub unknown1: f32,
    pub unknown2: f32,
    pub unknown3: f32,

    pub layer_change_room_id: u32,
    pub layer_change_layer_id: u32,
    pub item_id: u32,

    pub unknown4: u8,
    pub unknown5: f32,

    // "Used by SpinnerController"
    pub unknown6: u32,
    pub unknown7: u32,
    pub unknown8: u32,
}

impl<'r> SclyPropertyData for SpecialFunction<'r>
{
    const OBJECT_TYPE: u8 = 0x3A;
}
