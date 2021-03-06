use crate::blob::Blob;
use crate::codes::*;
use crate::tables::*;
use crate::types::*;
use crate::*;

use proc_macro2::TokenStream;
use quote::{format_ident, quote};

use std::iter::FromIterator;

#[derive(Debug, Clone, PartialEq, PartialOrd, Eq, Ord)]
pub struct TypeName {
    pub namespace: String,
    pub name: String,
    pub generics: Vec<TypeKind>,
    pub def: TypeDef,
}

impl TypeName {
    pub fn guid(&self, reader: &TypeReader, generics: bool) -> TypeGuid {
        if self.generics.is_empty() || generics {
            return TypeGuid::from_type_def(reader, self.def);
        }

        let mut data = vec![
            0x11, 0xf4, 0x7a, 0xd5, 0x7b, 0x73, 0x42, 0xc0, 0xab, 0xae, 0x87, 0x8b, 0x1e, 0x16,
            0xad, 0xee,
        ];
        data.extend_from_slice(self.interface_signature(reader).as_bytes());

        let mut hash = sha1::Sha1::new();
        hash.update(&data);
        let bytes = hash.digest().bytes();

        let first = u32::from_be_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        let second = u16::from_be_bytes([bytes[4], bytes[5]]);
        let mut third = u16::from_be_bytes([bytes[6], bytes[7]]);

        third = (third & 0x0fff) | (5 << 12);
        let fourth = (bytes[8] & 0x3f) | 0x80;

        TypeGuid([
            GuidConstant::U32(first),
            GuidConstant::U16(second),
            GuidConstant::U16(third),
            GuidConstant::U8(fourth),
            GuidConstant::U8(bytes[9]),
            GuidConstant::U8(bytes[10]),
            GuidConstant::U8(bytes[11]),
            GuidConstant::U8(bytes[12]),
            GuidConstant::U8(bytes[13]),
            GuidConstant::U8(bytes[14]),
            GuidConstant::U8(bytes[15]),
        ])
    }

    pub fn interface_signature(&self, reader: &TypeReader) -> String {
        let guid = TypeGuid::from_type_def(reader, self.def);

        if self.generics.is_empty() {
            format!("{{{:#?}}}", guid)
        } else {
            let mut result = format!("pinterface({{{:#?}}}", guid);

            for generic in &self.generics {
                result.push(';');
                result.push_str(&generic.signature(reader));
            }

            result.push(')');
            result
        }
    }

    pub fn class_signature(&self, reader: &TypeReader) -> String {
        let mut map = RequiredInterfaces::default();
        map.insert_required(reader, self);
        let default = map
            .0
            .into_iter()
            .find(|(_, kind)| *kind == InterfaceKind::Default)
            .unwrap()
            .0;

        format!(
            "rc({}.{};{})",
            self.namespace,
            self.name,
            default.interface_signature(reader)
        )
    }

    pub fn enum_signature(&self, reader: &TypeReader) -> String {
        format!(
            "enum({}.{};{})",
            self.namespace,
            self.name,
            self.enum_type(reader)
        )
    }

    fn enum_type(&self, reader: &TypeReader) -> &str {
        for field in self.def.fields(reader) {
            for constant in field.constants(reader) {
                match constant.value_type(reader) {
                    0x08 => return "i4",
                    0x09 => return "u4",
                    _ => panic!("Invalid enum type"),
                };
            }
        }

        panic!("Invalid enum");
    }

    pub fn struct_signature(&self, reader: &TypeReader) -> String {
        let mut result = format!("struct({}.{}", self.namespace, self.name);

        for field in self.def.fields(reader) {
            result.push(';');
            result.push_str(&TypeKind::from_field(reader, field).signature(reader));
        }

        result.push(')');
        result
    }

    pub fn delegate_signature(&self, reader: &TypeReader) -> String {
        if self.generics.is_empty() {
            format!("delegate({})", self.interface_signature(reader))
        } else {
            self.interface_signature(reader)
        }
    }

    pub fn from_type_def_or_ref(
        reader: &TypeReader,
        code: TypeDefOrRef,
        generics: &Vec<TypeKind>,
    ) -> Self {
        match code {
            TypeDefOrRef::TypeRef(value) => Self::from_type_ref(reader, value),
            TypeDefOrRef::TypeDef(value) => Self::from_type_def(reader, value),
            TypeDefOrRef::TypeSpec(value) => Self::from_type_spec(reader, value, generics),
        }
    }

    pub fn from_type_ref(reader: &TypeReader, type_ref: TypeRef) -> TypeName {
        let (namespace, name) = type_ref.name(reader);
        Self::from_type_def(reader, reader.resolve_type_def((namespace, name)))
    }

    pub fn from_type_def(reader: &TypeReader, def: TypeDef) -> Self {
        let (namespace, name) = def.name(reader);
        let namespace = namespace.to_string();
        let name = name.to_string();
        let mut generics = Vec::new();

        for generic in def.generics(reader) {
            let name = generic.name(reader).to_string();
            generics.push(TypeKind::Generic(name));
        }

        Self {
            namespace,
            name,
            generics,
            def,
        }
    }

    pub fn from_type_spec_blob(blob: &mut Blob, generics: &Vec<TypeKind>) -> Self {
        blob.read_unsigned();
        let def = TypeDefOrRef::decode(blob.read_unsigned(), blob.file_index).resolve(blob.reader);
        let mut args = Vec::with_capacity(blob.read_unsigned() as usize);

        for _ in 0..args.capacity() {
            args.push(TypeKind::from_blob(blob, generics));
        }
        let (namespace, name) = def.name(blob.reader);
        let namespace = namespace.to_string();
        let name = name.to_string();
        let generics = args;

        Self {
            namespace,
            name,
            generics,
            def,
        }
    }

    pub fn from_type_spec(reader: &TypeReader, spec: TypeSpec, generics: &Vec<TypeKind>) -> Self {
        let mut blob = spec.sig(reader);
        blob.read_unsigned();
        TypeName::from_type_spec_blob(&mut blob, generics)
    }

    pub fn runtime_name(&self) -> String {
        let mut result = format!("{}.{}", self.namespace, self.name);
        let mut generics = self.generics.iter();

        let first = match generics.next() {
            Some(first) => first,
            None => return result,
        };

        result += "<";
        result += &first.runtime_name();

        for kind in generics {
            result += ", ";
            result += &kind.runtime_name();
        }

        result += ">";

        result
    }

    pub fn dependencies(&self) -> Vec<TypeDef> {
        std::iter::once(self.def)
            .chain(self.generics.iter().flat_map(|i| i.dependencies()))
            .collect()
    }

    pub fn to_tokens(&self, calling_namespace: &str) -> TokenStream {
        let namespace = to_namespace_tokens(&self.namespace, calling_namespace);

        if self.generics.is_empty() {
            let name = format_ident(&self.name);
            quote! { #namespace#name }
        } else {
            let name = format_ident(&self.name[..self.name.len() - 2]);
            let generics = self.generics.iter().map(|g| g.to_tokens(calling_namespace));
            quote! { #namespace#name::<#(#generics),*> }
        }
    }

    pub fn to_abi_tokens(&self, calling_namespace: &str) -> TokenStream {
        let namespace = to_namespace_tokens(&self.namespace, calling_namespace);

        if self.generics.is_empty() {
            let name = format_abi_ident(&self.name);
            quote! { #namespace#name }
        } else {
            let name = format_abi_ident(&self.name[..self.name.len() - 2]);
            let generics = self.generics.iter().map(|g| g.to_tokens(calling_namespace));
            quote! { #namespace#name::<#(#generics),*> }
        }
    }

    // Note: ideally to_definition_tokens and to_abi_definiton_tokens would not be required
    // and we would simply use to_tokens and to_abi_tokens everywhere but Rust is really
    // weird in requiring `IVector<T>` in some places and `IVector::<T>` in others.
    pub fn to_definition_tokens(&self, calling_namespace: &str) -> TokenStream {
        if self.generics.is_empty() {
            let name = format_ident(&self.name);
            quote! { #name }
        } else {
            let name = format_ident(&self.name[..self.name.len() - 2]);
            let generics = self.generics.iter().map(|g| g.to_tokens(calling_namespace));
            quote! { #name<#(#generics),*> }
        }
    }

    pub fn to_abi_definition_tokens(&self, calling_namespace: &str) -> TokenStream {
        if self.generics.is_empty() {
            let name = format_abi_ident(&self.name);
            quote! { #name }
        } else {
            let name = format_abi_ident(&self.name[..self.name.len() - 2]);
            let generics = self.generics.iter().map(|g| g.to_tokens(calling_namespace));
            quote! { #name<#(#generics),*> }
        }
    }

    pub fn phantoms(&self) -> TokenStream {
        if self.generics.is_empty() {
            return TokenStream::new();
        }

        let phantoms = self.generics.iter().enumerate().map(|(count, generic)| {
            let name = format_ident!("__{}", count);
            let generic = generic.to_tokens("");
            quote! { #name: ::std::marker::PhantomData::<#generic>, }
        });

        TokenStream::from_iter(phantoms)
    }

    pub fn constraints(&self) -> TokenStream {
        let generics = self.generics.iter().map(|generic| {
            let generic = generic.to_tokens("");
            quote! { #generic: ::winrt::RuntimeType + 'static, }
        });

        TokenStream::from_iter(generics)
    }
}

fn format_abi_ident(name: &str) -> proc_macro2::Ident {
    quote::format_ident!("abi_{}", name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file::TableIndex;
    use crate::row::Row;

    #[test]
    fn runtime_name() {
        let mut type_name = TypeName {
            name: String::from("MyType"),
            namespace: String::from("Outer.Inner"),
            generics: vec![],
            def: TypeDef(Row {
                index: 0,
                table_index: TableIndex::InterfaceImpl,
                file_index: 0,
            }),
        };

        assert_eq!(type_name.runtime_name(), String::from("Outer.Inner.MyType"));

        type_name.generics = vec![TypeKind::Bool];

        assert_eq!(
            type_name.runtime_name(),
            String::from("Outer.Inner.MyType<Boolean>")
        );

        type_name.generics = vec![TypeKind::Bool, TypeKind::U8];

        assert_eq!(
            type_name.runtime_name(),
            String::from("Outer.Inner.MyType<Boolean, UInt8>")
        );
    }

    #[test]
    fn guids() {
        let reader = &TypeReader::from_os();

        // Non-generic interface guid
        let def = reader.resolve_type_def(("Windows.Foundation", "IAsyncAction"));
        let name = def.into_type(reader).name().clone();
        assert!(
            format!("{{{:#?}}}", name.guid(reader, false))
                == "{5a648006-843a-4da9-865b-9d26e5dfad7b}"
        );

        // Generic interface guid
        let stringable = reader.resolve_type_def(("Windows.Foundation", "IStringable"));
        let stringable = stringable.into_type(reader).name().clone();
        let def = reader.resolve_type_def(("Windows.Foundation.Collections", "IVector`1"));
        let mut name = def.into_type(reader).name().clone();
        name.generics.clear();
        name.generics.push(TypeKind::Interface(stringable));
        assert!(
            format!("{{{:#?}}}", name.guid(reader, false))
                == "{14b954c2-2914-530e-84a7-9473e2fb24e2}"
        );

        // Generic interface guid
        let stringable = reader.resolve_type_def(("Windows.Foundation", "IWwwFormUrlDecoderEntry"));
        let stringable = stringable.into_type(reader).name().clone();
        let def = reader.resolve_type_def(("Windows.Foundation.Collections", "IVectorView`1"));
        let mut name = def.into_type(reader).name().clone();
        name.generics.clear();
        name.generics.push(TypeKind::Interface(stringable));
        let guid = name.guid(reader, false);
        assert!(format!("{{{:#?}}}", guid) == "{b1f00d3b-1f06-5117-93ea-2a0d79116701}");

        // Unspecialized generic guid
        let def = reader.resolve_type_def(("Windows.Foundation.Collections", "IVector`1"));
        let name = def.into_type(reader).name().clone();
        assert!(
            format!("{{{:#?}}}", name.guid(reader, true))
                == "{913337e9-11a1-4345-a3a2-4e7f956e222d}"
        );
    }

    #[test]
    fn signatures() {
        let reader = &TypeReader::from_os();

        // Primitive signatures
        assert!(TypeKind::Bool.signature(reader) == "b1");
        assert!(TypeKind::Char.signature(reader) == "c2");
        assert!(TypeKind::I8.signature(reader) == "i1");
        assert!(TypeKind::U8.signature(reader) == "u1");
        assert!(TypeKind::I16.signature(reader) == "i2");
        assert!(TypeKind::U16.signature(reader) == "u2");
        assert!(TypeKind::I32.signature(reader) == "i4");
        assert!(TypeKind::U32.signature(reader) == "u4");
        assert!(TypeKind::I64.signature(reader) == "i8");
        assert!(TypeKind::U64.signature(reader) == "u8");
        assert!(TypeKind::F32.signature(reader) == "f4");
        assert!(TypeKind::F64.signature(reader) == "f8");
        assert!(TypeKind::String.signature(reader) == "string");
        assert!(TypeKind::Object.signature(reader) == "cinterface(IInspectable)");
        assert!(TypeKind::Guid.signature(reader) == "g16");

        // Non-generic interface signature
        let def = reader.resolve_type_def(("Windows.Foundation", "IAsyncAction"));
        let name = def.into_type(reader).name().clone();
        assert!(
            TypeKind::Interface(name).signature(reader) == "{5a648006-843a-4da9-865b-9d26e5dfad7b}"
        );

        // Generic interface signature
        let def = reader.resolve_type_def(("Windows.Foundation.Collections", "IVector`1"));
        let mut name = def.into_type(reader).name().clone();
        name.generics.clear();
        name.generics.push(TypeKind::I32);
        assert!(
            TypeKind::Interface(name).signature(reader)
                == "pinterface({913337e9-11a1-4345-a3a2-4e7f956e222d};i4)"
        );

        // Signed enum signature
        let def = reader.resolve_type_def(("Windows.Foundation", "AsyncStatus"));
        let name = def.into_type(reader).name().clone();
        assert!(
            TypeKind::Enum(name).signature(reader) == "enum(Windows.Foundation.AsyncStatus;i4)"
        );

        // Unsigned enum signature
        let def = reader.resolve_type_def((
            "Windows.ApplicationModel.Appointments",
            "AppointmentDaysOfWeek",
        ));
        let name = def.into_type(reader).name().clone();
        assert!(
            TypeKind::Enum(name).signature(reader)
                == "enum(Windows.ApplicationModel.Appointments.AppointmentDaysOfWeek;u4)"
        );

        // Non-generic delegate signature
        let def = reader.resolve_type_def(("Windows.Foundation", "AsyncActionCompletedHandler"));
        let name = def.into_type(reader).name().clone();
        assert!(
            TypeKind::Delegate(name).signature(reader)
                == "delegate({a4ed5c81-76c9-40bd-8be6-b1d90fb20ae7})"
        );

        // Generic delegate signature
        let stringable = reader.resolve_type_def(("Windows.Foundation", "IStringable"));
        let stringable = stringable.into_type(reader).name().clone();

        let def = reader.resolve_type_def(("Windows.Foundation", "EventHandler`1"));
        let mut name = def.into_type(reader).name().clone();
        name.generics.clear();
        name.generics.push(TypeKind::Interface(stringable));
        assert!(
            TypeKind::Delegate(name).signature(reader) == "pinterface({9de1c535-6ae1-11e0-84e1-18a905bcc53f};{96369f54-8eb6-48f0-abce-c1b211e627c3})"
        );

        // Class signature
        let def = reader.resolve_type_def(("Windows.Foundation", "Uri"));
        let name = def.into_type(reader).name().clone();
        assert!(
            TypeKind::Class(name).signature(reader)
                == "rc(Windows.Foundation.Uri;{9e365e57-48b2-4160-956f-c7385120bbfc})"
        );

        // Class with generic default interface signature
        let def = reader.resolve_type_def(("Windows.Foundation", "WwwFormUrlDecoder"));
        let name = def.into_type(reader).name().clone();
        assert!(
            TypeKind::Class(name).signature(reader)
                == "rc(Windows.Foundation.WwwFormUrlDecoder;{d45a0451-f225-4542-9296-0e1df5d254df})"
        );

        // Simple struct
        let def = reader.resolve_type_def(("Windows.Foundation", "Rect"));
        let name = def.into_type(reader).name().clone();
        assert!(
            TypeKind::Struct(name).signature(reader)
                == "struct(Windows.Foundation.Rect;f4;f4;f4;f4)"
        );
    }
}
