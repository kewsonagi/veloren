use super::area::LodArea;
use super::delta::LodDelta;
use super::index::{self, relative_to_1d, two_pow_u, AbsIndex, LodIndex};
use std::collections::HashMap;
use std::u32;
use vek::*;
/*
 Terminology:
 - Layer: the layer of the LoDTree, a tree can have n layers, every layer contains their child layer, except for the last one.
          Each layer contains a level, a number from 15-0. the level of each child must be lower than the parents layer!
 - Detail: Each Layer contains information about that layer, here called Detail. This is the usable information we want to store in the LoDTree
 - LodPos: A LodPos marks a specific position inside the LoDTree, but not their layer.
           Each Detail has a LodPos. Multiple Details can exist at the same LodPos on different layers!
 - Index: This refers to the actually storage for the index for the next layer (often a u16,u32)
 - Key: always refers to the storage of a LAYER. Any keyword with KEY is either of type usize or LodPos.
 - Prefix P always means parent, Prexix C always child, no prefix means for this layer.

 traits:
 - IndexStore: Every layer must implement this for either KEY = usize or KEY = LodPos and INDEX is often u16/u32. depending on the store of the parent detail.
               It is accessed by parent layer to store the index when a detail is added or removed.
               Every IndexStore has a parent, and a constant OWN_PER_PARENT which says, how many details fit into one element of the parent.
 - DetailStore: Every layer must implement this for either KEY = usize or KEY = LodPos, independent from the parent.
                This is used to store the actual detail of every layer.
 - Nestable: All layers, except the lowest one implement this trait. It links the below layer to interact with the child layer.
 !!Calculations will be implemented on these 3 Stores, rather than the actual structs to reduce duplciate coding!!
 - Traversable: trait is used to get child layer and child Index for a concrete position.
 - Materializeable: trait is used to actually return a Detail for a concrete position.

 Actual structs regarding of position in the chain. They represent the Layers and contain the Details, they implement (some of) the 3 Store traits
 Naming Scheme is <Own Detail Type><Parent Detail Type>[Nest]Layer
 - VecVecLayer/VecHashLayer: Vec Leaf Layers that have a vec/hash index and dont have a child layer.
 - VecVecNestLayer/VecHashNestLayer: Vec Layers that have a vec/hash index and are middle layers
 - HashNoneNestLayer: Hash Layer that has no index and must be parent layer

 Result Structs:
 - LayerResult: Is used to access a layer meta information or Detail via LoDTree.traverse().get().get().get().mat().
                When LoDTree.traverse() returns a LayerResult.
*/

pub type LodPos = LodIndex;

pub trait KeyConvertable {
    type KEY;
    type INDEX;
    fn uncompress(&self, index: &Self::INDEX) -> Self::KEY;
    fn compress(&self, key: &Self::KEY) -> Self::INDEX;
}

pub trait IndexStore {
    type PARENT_DETAIL: DetailStore;
    type INDEX: Copy;
    const OWN_PER_PARENT: usize; // theoretically this can be auto calculated in a new trait, however rust ist detecting conflicting implementations

    fn load(&self, key: Self::PARENT_DETAIL::KEY) -> Self::INDEX;
    fn store(&mut self, key: Self::PARENT_DETAIL::KEY, index: Self::INDEX);
}
// Every IndexStore practivally needs this, but i want to autocalculte OWN_PER_PARENT so this needs to be an extra trait
pub trait IndexStoreLevel: IndexStore {


}
pub trait DetailStore {
    type KEY;
    type DETAIL;
    const LEVEL: u8;

    fn load(&self, key: Self::KEY) -> &Self::DETAIL;
    fn load_mut(&mut self, key: Self::KEY) -> &mut Self::DETAIL;
    fn store(&mut self, key: Self::KEY, detail: Self::DETAIL);
}

pub trait Nestable {
    type NESTED: IndexStore + DetailStore;

    fn nested(&self) -> &Self::NESTED;
}

//TODO: make LodTree trait and make traverse a function which returns a LayerResult to the TOP Layer (and not one layer below that), or call it iter, lets see

pub trait Traversable<C> {
    fn get(self) -> C;
}
pub trait Materializeable<T> {
    fn mat(self) -> T;
}

//CK is childs key of IndexStore, its needed for Traversable, but IndexStore cannot be a dependency, because of last node which acets materializeable but not Traversable
pub struct LayerResult<'a, N: DetailStore> {
    child: &'a N,
    wanted: LodPos,
    key: N::KEY,
}

//TODO: optimize this self away! vtable
//TODO: arrr and the clone
impl KeyConvertable<KEY=usize, INDEX=u16> {
    fn uncompress(&self, index: &u16) -> usize { index.clone() as usize }
    fn compress(&self, key: &usize) -> u16 { key.clone() as u16 }
}
impl KeyConvertable<KEY=usize, INDEX=u32> {
    fn uncompress(&self, index: &u32) -> usize { index.clone() as usize }
    fn compress(&self, key: &usize) -> u32 { key.clone() as u32 }
}
impl KeyConvertable<KEY=LodPos, INDEX=LodPos> {
    fn uncompress(&self, index: &LodPos) -> LodPos { index.clone() }
    fn compress(&self, key: &LodPos) -> LodPos { key.clone() }
}

//#######################################################

pub struct VecVecLayer<T, PI: Copy, const L: u8> {
    pub detail: Vec<T>,
    pub index: Vec<PI>,
}
pub struct VecHashLayer<T, PI: Copy, const L: u8> {
    pub detail: Vec<T>,
    pub index: HashMap<LodPos, PI>,
}

//T: own detail type
//PI: parents index type u16, u32
pub struct VecVecNestLayer<N: IndexStore + DetailStore, T, PI: Copy, const L: u8> {
    pub detail: Vec<T>,
    pub index: Vec<PI>,
    pub nested: N,
}

pub struct VecHashNestLayer<N: IndexStore + DetailStore, T, PI: Copy, const L: u8> {
    pub detail: Vec<T>,
    pub index: HashMap<LodPos, PI>,
    pub nested: N,
}

pub struct HashNoneNestLayer<N: IndexStore + DetailStore, T, const L: u8> {
    pub detail: HashMap<LodPos, T>,
    pub nested: N,
}

#[rustfmt::skip]
impl<T, P: , const L: u8> DetailStore for VecVecLayer<T, PI, { L }> {
    type PARENT_DETAIL = usize; type DETAIL=T; const LEVEL: u8 = { L };
    fn load(&self, key: usize) -> &T {  self.detail.get(key).unwrap() }
    fn load_mut(&mut self, key: usize) -> &mut T {  self.detail.get_mut(key).unwrap() }
    fn store(&mut self, key: usize, detail: T) { self.detail.insert(key, detail); }
}
#[rustfmt::skip]
impl<N: IndexStore<KEY = usize> + DetailStore, T, PI: Copy, const L: u8> DetailStore for VecVecNestLayer<N, T, PI, { L }>  {
    type PARENT_DETAIL = usize; type DETAIL=T; const LEVEL: u8 = { L };
    fn load(&self, key: usize) -> &T { self.detail.get(key).unwrap() }
    fn load_mut(&mut self, key: usize) -> &mut T {  self.detail.get_mut(key).unwrap() }
    fn store(&mut self, key: usize, detail: T) { self.detail.insert(key, detail); }
}
#[rustfmt::skip]
impl<T, PI: Copy, const L: u8> DetailStore for VecHashLayer<T, PI, { L }> {
    type KEY = usize; type DETAIL=T; const LEVEL: u8 = { L };
    fn load(&self, key: usize) -> &T { self.detail.get(key).unwrap() }
    fn load_mut(&mut self, key: usize) -> &mut T {  self.detail.get_mut(key).unwrap() }
    fn store(&mut self, key: usize, detail: T) { self.detail.insert(key, detail); }
}
#[rustfmt::skip]
impl<N: IndexStore<KEY = usize> + DetailStore, T, PI: Copy, const L: u8> DetailStore for VecHashNestLayer<N, T, PI, { L }>  {
    type KEY = usize; type DETAIL=T; const LEVEL: u8 = { L };
    fn load(&self, key: usize) -> &T { self.detail.get(key).unwrap() }
    fn load_mut(&mut self, key: usize) -> &mut T {  self.detail.get_mut(key).unwrap() }
    fn store(&mut self, key: usize, detail: T) { self.detail.insert(key, detail); }
}
#[rustfmt::skip]
impl<N: IndexStore<KEY = LodPos> + DetailStore, T, const L: u8> DetailStore for HashNoneNestLayer<N, T, { L }>  {
    type KEY = LodPos; type DETAIL=T; const LEVEL: u8 = { L };
    fn load(&self, key: LodPos) -> &T { self.detail.get(&key).unwrap() }
    fn load_mut(&mut self, key: LodPos) -> &mut T {  self.detail.get_mut(&key).unwrap() }
    fn store(&mut self, key: LodPos, detail: T) { self.detail.insert(key, detail); }
}

#[rustfmt::skip]
impl<T, PI: Copy, const L: u8> IndexStore for VecVecLayer<T, PI, { L }> {
    type KEY = usize; type INDEX=PI; const OWN_PER_PARENT: usize = 4096; //TODO: calculate these correctly
    fn load(&self, key: usize) -> PI {  *self.index.get(key).unwrap() }
    fn store(&mut self, key: usize, index: PI) { self.index.insert(key, index); }
}
#[rustfmt::skip]
impl<N: IndexStore<KEY = usize> + DetailStore, T, PI: Copy, const L: u8> IndexStore for VecVecNestLayer<N, T, PI, { L }> {
    type KEY = usize; type INDEX=PI; const OWN_PER_PARENT: usize = 32768;
    fn load(&self, key: usize) -> PI { *self.index.get(key).unwrap() }
    fn store(&mut self, key: usize, index: PI) { self.index.insert(key, index); }
}
#[rustfmt::skip]
impl<T, PI: Copy, const L: u8> IndexStore for VecHashLayer<T, PI, { L }> {
    type KEY = LodPos; type INDEX=PI; const OWN_PER_PARENT: usize = 1337;
    fn load(&self, key: LodPos) -> PI { *self.index.get(&key).unwrap() }
    fn store(&mut self, key: LodPos, index: PI) { self.index.insert(key, index); }
}
#[rustfmt::skip]
impl<N: IndexStore<KEY = usize> + DetailStore, T, PI: Copy, const L: u8> IndexStore for VecHashNestLayer<N, T, PI, { L }>  {
    type KEY = LodPos; type INDEX=PI; const OWN_PER_PARENT: usize = 4096;
    fn load(&self, key: LodPos) -> PI { *self.index.get(&key).unwrap() }
    fn store(&mut self, key: LodPos, index: PI) { self.index.insert(key, index); }
}

#[rustfmt::skip]
impl<N: IndexStore<KEY = usize> + DetailStore, T, PI: Copy, const L: u8> Nestable for VecVecNestLayer<N, T, PI, { L }>  {
    type NESTED=N;
    fn nested(&self) -> &N { &self.nested }
}
#[rustfmt::skip]
impl<N: IndexStore<KEY = usize> + DetailStore, T, PI: Copy, const L: u8> Nestable for VecHashNestLayer<N, T, PI, { L }>  {
    type NESTED=N;
    fn nested(&self) -> &N { &self.nested }
}
#[rustfmt::skip]
impl<N: IndexStore<KEY = LodPos> + DetailStore, T, const L: u8> Nestable for HashNoneNestLayer<N, T, { L }>  {
    type NESTED=N;
    fn nested(&self) -> &N { &self.nested }
}

//#######################################################

impl<NC: IndexStore<KEY = LodPos> + DetailStore, T, const L: u8> HashNoneNestLayer<NC, T, { L }>
{
    pub fn trav<'a>(&'a self, pos: LodPos) -> LayerResult<'a, Self> {
        LayerResult {
            child: self,
            wanted: pos,
            key: pos.align_to_layer_id(Self::LEVEL),
        }
    }
}

/*impl<'a, N: DetailStore + Nestable>
Traversable<LayerResult<'a, N::NESTED, <N::NESTED as IndexStore>::KEY>> for LayerResult<'a, N, <N::NESTED as IndexStore>::KEY>
where N::NESTED: IndexStore,
      <N::NESTED as IndexStore>::KEY: Copy,
      <N::NESTED as IndexStore>::KEY: KeyConvertable<KEY=<N::NESTED as IndexStore>::KEY, INDEX=<N::NESTED as IndexStore>::INDEX>
{
    fn get(self) -> LayerResult<'a, N::NESTED, <N::NESTED as IndexStore>::KEY> {
        println!("{}", N::LEVEL);
        let child = self.child.nested();
        let key = self.key;
        //let index = self.index.align_to_layer_id(N::LEVEL);
        LayerResult {
            child,
            wanted: self.wanted,
            key: key.uncompress(&IndexStore::load(child, key)),
        }
    }
}*/


impl<'a, N: DetailStore + Nestable>
Traversable<LayerResult<'a, N::NESTED>> for LayerResult<'a, N>
where N::NESTED: IndexStore,
      <N::NESTED as IndexStore>::KEY: Copy,
      <N as DetailStore>::KEY: KeyConvertable<KEY=<N::NESTED as DetailStore>::KEY, INDEX=<N::NESTED as IndexStore>::INDEX>,
// ERROR PRATISCH GESEHEN GILT DIE FOLGENDE ZEILE IMMER, aber das sieht der compiler nur wenn wir auf den structs arbeiten,
// AUF DEN TRAITS WEIS ER ES NICHT! DAS IST KAKA
      <N::NESTED as IndexStore>::KEY == <N as DetailStore>::KEY
{
    fn get(self) -> LayerResult<'a, N::NESTED> {
        println!("{}", N::LEVEL);
        let child = self.child.nested();
        let key = self.key;
        //let index = self.index.align_to_layer_id(N::LEVEL);
        LayerResult {
            child,
            wanted: self.wanted,
            key: key.uncompress(&IndexStore::load(child, key)),
        }
    }
}

impl<'a, N: IndexStore + DetailStore> Materializeable<N::DETAIL> for LayerResult<'a, N> {
    fn mat(self) -> N::DETAIL {
        unimplemented!();
    }
}

#[rustfmt::skip]
pub type ExampleDelta =
    HashNoneNestLayer<
        VecHashNestLayer<
            VecVecNestLayer<
                VecVecLayer<
                    (), u16, 0
                > ,(), u32, 4
            > ,Option<()> , u16, 9
        > ,() , 13
    >;

#[cfg(test)]
mod tests {
    use crate::lodstore::newdata::*;

    #[test]
    fn newdata() {
        let x = ExampleDelta {
            detail: HashMap::new(),
            nested: VecHashNestLayer {
                detail: Vec::new(),
                index: HashMap::new(),
                nested: VecVecNestLayer {
                    detail: Vec::new(),
                    index: Vec::new(),
                    nested: VecVecLayer {
                        detail: Vec::new(),
                        index: Vec::new(),
                    },
                },
            },
        };
        let i = LodPos::new(Vec3::new(0, 1, 2));
        let y = x.trav(i);
        let ttc = y.get().get().get();
        let tt = ttc.mat();
    }
}

// TODO: instead of storing the absolute index in index, we store (index / number of entities), which means a u16 in Block can not only hold 2 full Subblocks (32^3 subblocks per block). but the full 2^16-1 ones.