
extern crate serde;
extern crate serde_json;
#[macro_use]
extern crate serde_derive;
extern crate specs;

use serde::{Serialize, Serializer, Deserialize, Deserializer};
use serde::de::DeserializeSeed;
use specs::{Gate, Join};

use std::marker::PhantomData;

pub trait Group {
    /// Components defined in this group, not a subgroup.
    fn local_components() -> Vec<&'static str>;
    /// Components defined in this group along with subgroups.
    fn components() -> Vec<&'static str>;
    /// Subgroups included in this group.
    fn subgroups() -> Vec<&'static str>;
    /// Serializes the group of components from the world.
    fn serialize_group<S: Serializer>(world: &specs::World, serializer: S) -> Result<S::Ok, S::Error>;
    /// Helper method for serializing the world.
    fn serialize_subgroup<S: Serializer>(world: &specs::World, map: &mut S::SerializeMap) -> Result<(), S::Error>;
    /// Deserializes the group of components into the world.
    fn deserialize_group<D: Deserializer>(world: &mut specs::World, entities: &Vec<specs::Entity>, deserializer: D) -> Result<(), D::Error>;
    /// Helper method for deserializing the world.
    fn deserialize_subgroup<V>(world: &mut specs::World, entities: &Vec<specs::Entity>, key: String, visitor: &mut V) -> Result<Option<()>, V::Error>
        where V: serde::de::MapVisitor;
}

macro_rules! group {
    ( $name:ident => { $( $subgroup:path, )* } [ $( $component:path, )* ] ) => {
        struct $name;

        impl Group for $name {
            fn local_components() -> Vec<&'static str> {
                vec![ $( stringify!($component), )* ]
            }
            fn components() -> Vec<&'static str> {
                let mut list = <$name as Group>::local_components();
                $(
                    for component in <$subgroup as Group>::components() {
                        list.push(component);
                    }
                )*
                list
            }
            fn subgroups() -> Vec<&'static str> {
                vec![ $( stringify!($subgroup), )* ]
            }
            fn serialize_group<S: Serializer>(world: &specs::World, serializer: S) -> Result<S::Ok, S::Error> {
                use serde::ser::SerializeMap;
                let mut map = serializer.serialize_map(None)?;
                $(
                    map.serialize_key(stringify!($component))?;
                    let storage = world.read::<$component>().pass();
                    map.serialize_value(&storage)?;
                )*

                $(
                    <$subgroup as Group>::serialize_subgroup::<S>(world, &mut map)?;
                )*

                map.end()
            }

            fn serialize_subgroup<S: Serializer>(world: &specs::World, map: &mut S::SerializeMap) -> Result<(), S::Error> {
                use serde::ser::SerializeMap;
                $(
                    map.serialize_key(stringify!($component))?;
                    let storage = world.read::<$component>().pass();
                    map.serialize_value(&storage)?;
                )*

                $(
                    <$subgroup as Group>::serialize_subgroup::<S>(world, map)?;
                )*

                Ok(())
            }

            fn deserialize_group<D: Deserializer>(world: &mut specs::World, entities: &Vec<specs::Entity>, deserializer: D) -> Result<(), D::Error> {
                use std::fmt;
                use specs::PackedData;

                struct ComponentVisitor<'a>(&'a mut specs::World, &'a Vec<specs::Entity>);
                impl<'a> serde::de::Visitor for ComponentVisitor<'a> {
                    type Value = ();
                    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                        write!(formatter, "a map of component identifiers to packed data")
                    }

                    fn visit_map<V>(self, mut visitor: V) -> Result<(), V::Error>
                        where V: serde::de::MapVisitor
                    {
                        while let Some(key) = visitor.visit_key::<String>()? {
                            match &*key {
                                $(
                                    stringify!($component) => {
                                        let mut storage = self.0.write::<$component>().pass();
                                        let packed = visitor.visit_value::<PackedData<$component>>()?;
                                        storage.merge(self.1, packed);
                                    },
                                )*
                                key @ _ => {
                                    $(
                                        if let Some(()) = <$subgroup as Group>::deserialize_subgroup(self.0, self.1, key.to_owned(), &mut visitor)? {
                                            continue; // subgroup deserialized the components
                                        }
                                    )*
                                    continue; // not in the registered component list, ignore
                                },
                            }
                        }

                        Ok(())
                    }
                }
                
                Ok(deserializer.deserialize_map(ComponentVisitor(world, entities))?)
            }

            fn deserialize_subgroup<V>(world: &mut specs::World, entities: &Vec<specs::Entity>, key: String, mut visitor: &mut V) -> Result<Option<()>, V::Error>
                where V: serde::de::MapVisitor
            {
                match &*key {
                    $(
                        stringify!($component) => {
                            let mut storage = world.write::<$component>().pass();
                            let packed = visitor.visit_value::<specs::PackedData<$component>>()?;
                            storage.merge(entities, packed);
                            Ok(Some(()))
                        },
                    )*
                    key @ _ => {
                        $(
                            if let Some(()) = <$subgroup as Group>::deserialize_subgroup(world, entities, key.to_owned(), visitor)? {
                                return Ok(Some(()));
                            }
                        )*
                        Ok(None)
                    },
                }
            }
        }
    }
}

struct WorldSerializer<'a, G: Group>(&'a specs::World, PhantomData<G>);
struct WorldDeserializer<'a, G: Group>(&'a mut specs::World, &'a Vec<specs::Entity>, PhantomData<G>);

impl<'a, G: Group> Serialize for WorldSerializer<'a, G> {
	fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where S: Serializer
    {
		<G as Group>::serialize_group(self.0, serializer)
	}
}

impl<'a, G: Group> DeserializeSeed for WorldDeserializer<'a, G> {
    type Value = ();
	fn deserialize<D>(self, deserializer: D) -> Result<(), D::Error>
        where D: Deserializer
    {
		<G as Group>::deserialize_group(self.0, self.1, deserializer)
	}
}

#[derive(Debug, Serialize, Deserialize)]
struct Comp1(u32);
impl specs::Component for Comp1 {
    type Storage = specs::VecStorage<Comp1>;
}

#[derive(Debug, Serialize, Deserialize)]
struct Comp2(String);
impl specs::Component for Comp2 {
    type Storage = specs::VecStorage<Comp2>;
}

#[derive(Debug, Serialize, Deserialize)]
struct Comp3(f32);
impl specs::Component for Comp3 {
    type Storage = specs::VecStorage<Comp3>;
}

pub mod test {
    #[derive(Debug, Serialize, Deserialize)]
    pub struct Comp4(pub bool);
    impl ::specs::Component for Comp4 {
        type Storage = ::specs::VecStorage<Comp4>;
    }
}

group!(amethyst => { } [ Comp1, ]);

#[derive(Debug, Serialize, Deserialize)]
struct TestComponent {
    field1: u32,
    field2: u64,
    field3: f32,
}

group!(Subgroup => { } [ Comp1, Comp2, ]);
group!(TestGroup => { Subgroup, } [ Comp3, test::Comp4, ]);

fn main() {
    let mut world = specs::World::new();
    world.register::<Comp1>();
    world.register::<Comp2>();
    world.register::<Comp3>();
    world.register::<test::Comp4>();
    world.create_now().with(Comp1(5)).with(test::Comp4(true)).build();
    world.create_now().with(Comp2("Some data".to_owned())).with(Comp3(5.5)).build();
    world.create_pure();
    world.create_now().with(Comp3(0.5)).build();
    world.create_pure();
    world.create_now().with(Comp3(3.14159265358979)).build();
    world.create_now().with(Comp1(0)).build();
    world.create_pure();
    world.create_now().with(Comp2("Some other data".to_owned())).with(Comp1(0)).build();
    world.create_now().with(Comp1(15)).build();

    let serialized = {
        let s: WorldSerializer<TestGroup> = WorldSerializer(&world, PhantomData);
        let result = serde_json::to_string_pretty(&s).unwrap();
        println!("result: {}", result);

        let test = r#"
            {
                "field3":0.5,
                "field1":15,
                "field2":250
            }
        "#;

        println!("local: {:?}", TestGroup::local_components());
        println!("all: {:?}", TestGroup::components());
        println!("subgroups: {:?}", TestGroup::subgroups());

        result
    };

    {
        let entities = world.create_iter().take(10).collect::<Vec<specs::Entity>>();
        let s: WorldDeserializer<TestGroup> = WorldDeserializer(&mut world, &entities, PhantomData);
        s.deserialize(&mut serde_json::de::Deserializer::from_str(&serialized));
    }

    {
        let entities = world.entities();
        let comp1s = world.read::<Comp1>().pass();
        let comp2s = world.read::<Comp2>().pass();
        let comp3s = world.read::<Comp3>().pass();

        for entity in (&entities).join() {
            println!("{:?}: {:?} {:?} {:?}", entity, comp1s.get(entity), comp2s.get(entity), comp3s.get(entity));
        }
    }
}
