//! Minimal HPROF writer for tests (64-bit ids, JAVA PROFILE 1.0.2).

pub struct HprofBuilder {
    records: Vec<u8>,
    heap: Vec<u8>,
}

impl HprofBuilder {
    pub fn new() -> Self {
        Self {
            records: Vec::new(),
            heap: Vec::new(),
        }
    }

    pub fn utf8(&mut self, id: u64, text: &str) -> &mut Self {
        let mut body = id.to_be_bytes().to_vec();
        body.extend_from_slice(text.as_bytes());
        write_record(0x01, &body, &mut self.records);
        self
    }

    pub fn load_class(&mut self, serial: u32, class_obj_id: u64, name_id: u64) -> &mut Self {
        let mut body = Vec::new();
        body.extend_from_slice(&serial.to_be_bytes());
        body.extend_from_slice(&class_obj_id.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&name_id.to_be_bytes());
        write_record(0x02, &body, &mut self.records);
        self
    }

    pub fn class_dump(
        &mut self,
        obj_id: u64,
        super_class: Option<u64>,
        instance_size: u32,
        instance_fields: &[(u64, u8)],
    ) -> &mut Self {
        let mut body = Vec::new();
        body.push(0x20);
        body.extend_from_slice(&obj_id.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        write_optional_id(&mut body, super_class);
        write_optional_id(&mut body, None);
        write_optional_id(&mut body, None);
        write_optional_id(&mut body, None);
        body.extend_from_slice(&0u64.to_be_bytes());
        body.extend_from_slice(&0u64.to_be_bytes());
        body.extend_from_slice(&instance_size.to_be_bytes());
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(&0u16.to_be_bytes());
        body.extend_from_slice(&(instance_fields.len() as u16).to_be_bytes());
        for &(name_id, type_byte) in instance_fields {
            body.extend_from_slice(&name_id.to_be_bytes());
            body.push(type_byte);
        }
        self.heap.extend_from_slice(&body);
        self
    }

    pub fn instance(&mut self, obj_id: u64, class_obj_id: u64, field_bytes: &[u8]) -> &mut Self {
        let mut body = Vec::new();
        body.push(0x21);
        body.extend_from_slice(&obj_id.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&class_obj_id.to_be_bytes());
        body.extend_from_slice(&(field_bytes.len() as u32).to_be_bytes());
        body.extend_from_slice(field_bytes);
        self.heap.extend_from_slice(&body);
        self
    }

    pub fn object_array(
        &mut self,
        obj_id: u64,
        array_class_obj_id: u64,
        elements: &[Option<u64>],
    ) -> &mut Self {
        let mut body = Vec::new();
        body.push(0x22);
        body.extend_from_slice(&obj_id.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&(elements.len() as u32).to_be_bytes());
        body.extend_from_slice(&array_class_obj_id.to_be_bytes());
        for elem in elements {
            write_optional_id(&mut body, *elem);
        }
        self.heap.extend_from_slice(&body);
        self
    }

    pub fn primitive_int_array(&mut self, obj_id: u64, values: &[i32]) -> &mut Self {
        let mut body = Vec::new();
        body.push(0x23);
        body.extend_from_slice(&obj_id.to_be_bytes());
        body.extend_from_slice(&0u32.to_be_bytes());
        body.extend_from_slice(&(values.len() as u32).to_be_bytes());
        body.push(0x0A);
        for v in values {
            body.extend_from_slice(&v.to_be_bytes());
        }
        self.heap.extend_from_slice(&body);
        self
    }

    pub fn gc_root_unknown(&mut self, obj_id: u64) -> &mut Self {
        let mut body = Vec::new();
        body.push(0xFF);
        body.extend_from_slice(&obj_id.to_be_bytes());
        self.heap.extend_from_slice(&body);
        self
    }

    pub fn build(mut self) -> Vec<u8> {
        if !self.heap.is_empty() {
            write_record(0x0C, &self.heap, &mut self.records);
        }
        let mut out = Vec::new();
        out.extend_from_slice(b"JAVA PROFILE 1.0.2\0");
        out.extend_from_slice(&8u32.to_be_bytes());
        out.extend_from_slice(&0u32.to_be_bytes());
        out.extend_from_slice(&1000u32.to_be_bytes());
        out.extend_from_slice(&self.records);
        out
    }
}

impl Default for HprofBuilder {
    fn default() -> Self {
        Self::new()
    }
}

fn write_record(tag: u8, body: &[u8], out: &mut Vec<u8>) {
    out.push(tag);
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(body);
}

fn write_optional_id(out: &mut Vec<u8>, id: Option<u64>) {
    out.extend_from_slice(&id.unwrap_or(0).to_be_bytes());
}

pub fn linked_list_hprof() -> Vec<u8> {
    const ID_OBJECT: u64 = 0x1000;
    const ID_NODE: u64 = 0x1001;
    const ID_FIELD_NEXT: u64 = 0x1002;
    const CLASS_OBJECT: u64 = 0x2000;
    const CLASS_NODE: u64 = 0x2001;
    const OBJ_ROOT: u64 = 0x3000;
    const OBJ_N1: u64 = 0x3001;
    const OBJ_N2: u64 = 0x3002;

    let mut b = HprofBuilder::new();
    b.utf8(ID_OBJECT, "java/lang/Object")
        .utf8(ID_NODE, "com/example/Node")
        .utf8(ID_FIELD_NEXT, "next")
        .load_class(1, CLASS_OBJECT, ID_OBJECT)
        .load_class(2, CLASS_NODE, ID_NODE)
        .class_dump(CLASS_OBJECT, None, 16, &[])
        .class_dump(CLASS_NODE, Some(CLASS_OBJECT), 24, &[(ID_FIELD_NEXT, 0x02)])
        .gc_root_unknown(OBJ_ROOT)
        .instance(OBJ_ROOT, CLASS_NODE, &OBJ_N1.to_be_bytes())
        .instance(OBJ_N1, CLASS_NODE, &OBJ_N2.to_be_bytes())
        .instance(OBJ_N2, CLASS_NODE, &0u64.to_be_bytes());
    b.build()
}

pub fn holder_and_array_hprof() -> Vec<u8> {
    const ID_OBJECT: u64 = 0x1000;
    const ID_HOLDER: u64 = 0x1001;
    const ID_ELEM: u64 = 0x1002;
    const CLASS_OBJECT: u64 = 0x2000;
    const CLASS_HOLDER: u64 = 0x2001;
    const CLASS_ELEM: u64 = 0x2002;
    const OBJ_ROOT: u64 = 0x3000;
    const OBJ_A: u64 = 0x3001;
    const OBJ_B: u64 = 0x3002;
    const OBJ_ARR: u64 = 0x3003;
    const OBJ_ORPHAN: u64 = 0x3004;

    let mut b = HprofBuilder::new();
    b.utf8(ID_OBJECT, "java/lang/Object")
        .utf8(ID_HOLDER, "com/example/Holder")
        .utf8(ID_ELEM, "com/example/Elem")
        .load_class(1, CLASS_OBJECT, ID_OBJECT)
        .load_class(2, CLASS_HOLDER, ID_HOLDER)
        .load_class(3, CLASS_ELEM, ID_ELEM)
        .class_dump(CLASS_OBJECT, None, 16, &[])
        .class_dump(CLASS_HOLDER, Some(CLASS_OBJECT), 16, &[])
        .class_dump(CLASS_ELEM, Some(CLASS_OBJECT), 16, &[])
        .gc_root_unknown(OBJ_ROOT)
        .instance(OBJ_ROOT, CLASS_HOLDER, &[])
        .instance(OBJ_A, CLASS_ELEM, &[])
        .instance(OBJ_B, CLASS_ELEM, &[])
        .instance(OBJ_ORPHAN, CLASS_ELEM, &[])
        .object_array(OBJ_ARR, CLASS_ELEM, &[Some(OBJ_A), Some(OBJ_B)])
        .primitive_int_array(0x4000, &[1, 2, 3]);
    b.build()
}

pub fn parse_fixture(bytes: &[u8]) -> jvm_hprof::Hprof<'_> {
    jvm_hprof::parse_hprof(bytes).expect("fixture hprof should parse")
}

/// Keeps HPROF bytes alive for the lifetime of parsed views.
pub struct OwnedFixture {
    bytes: Vec<u8>,
}

impl OwnedFixture {
    pub fn linked_list() -> Self {
        Self {
            bytes: linked_list_hprof(),
        }
    }

    pub fn holder_and_array() -> Self {
        Self {
            bytes: holder_and_array_hprof(),
        }
    }

    pub fn parse(&self) -> jvm_hprof::Hprof<'_> {
        parse_fixture(&self.bytes)
    }
}
