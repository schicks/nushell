use crate::{DeclId, ModuleId, OverlayId, Type, Value, VarId};
use std::borrow::Borrow;
use std::collections::HashMap;
use std::hash::{Hash, Hasher};

pub static DEFAULT_OVERLAY_NAME: &str = "zero";

/// Tells whether a decl is visible or not
#[derive(Debug, Clone)]
pub struct Visibility {
    decl_ids: HashMap<DeclId, bool>,
}

impl Visibility {
    pub fn new() -> Self {
        Visibility {
            decl_ids: HashMap::new(),
        }
    }

    pub fn is_decl_id_visible(&self, decl_id: &DeclId) -> bool {
        *self.decl_ids.get(decl_id).unwrap_or(&true) // by default it's visible
    }

    pub fn hide_decl_id(&mut self, decl_id: &DeclId) {
        self.decl_ids.insert(*decl_id, false);
    }

    pub fn use_decl_id(&mut self, decl_id: &DeclId) {
        self.decl_ids.insert(*decl_id, true);
    }

    /// Overwrite own values with the other
    pub fn merge_with(&mut self, other: Visibility) {
        self.decl_ids.extend(other.decl_ids);
    }

    /// Take new values from the other but keep own values
    pub fn append(&mut self, other: &Visibility) {
        for (decl_id, visible) in other.decl_ids.iter() {
            if !self.decl_ids.contains_key(decl_id) {
                self.decl_ids.insert(*decl_id, *visible);
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScopeFrame {
    /// List of both active and inactive overlays in this ScopeFrame.
    ///
    /// The order does not have any meaning. Indexed locally (within this ScopeFrame) by
    /// OverlayIds in active_overlays.
    pub overlays: Vec<(Vec<u8>, OverlayFrame)>,

    /// List of currently active overlays.
    ///
    /// Order is significant: The last item points at the last activated overlay.
    pub active_overlays: Vec<OverlayId>,

    /// Removed overlays from previous scope frames / permanent state
    pub removed_overlays: Vec<Vec<u8>>,

    /// temporary storage for predeclarations
    pub predecls: HashMap<Vec<u8>, DeclId>,
}

impl ScopeFrame {
    pub fn new() -> Self {
        Self {
            overlays: vec![],
            active_overlays: vec![],
            removed_overlays: vec![],
            predecls: HashMap::new(),
        }
    }

    pub fn with_empty_overlay(name: Vec<u8>, origin: ModuleId, prefixed: bool) -> Self {
        Self {
            overlays: vec![(name, OverlayFrame::from_origin(origin, prefixed))],
            active_overlays: vec![0],
            removed_overlays: vec![],
            predecls: HashMap::new(),
        }
    }

    pub fn get_var(&self, var_name: &[u8]) -> Option<&VarId> {
        for overlay_id in self.active_overlays.iter().rev() {
            if let Some(var_id) = self
                .overlays
                .get(*overlay_id)
                .expect("internal error: missing overlay")
                .1
                .vars
                .get(var_name)
            {
                return Some(var_id);
            }
        }

        None
    }

    pub fn active_overlay_ids(&self, removed_overlays: &mut Vec<Vec<u8>>) -> Vec<OverlayId> {
        for name in &self.removed_overlays {
            if !removed_overlays.contains(name) {
                removed_overlays.push(name.clone());
            }
        }

        self.active_overlays
            .iter()
            .filter(|id| !removed_overlays.contains(self.get_overlay_name(**id)))
            .copied()
            .collect()
    }

    pub fn active_overlays<'a, 'b>(
        &'b self,
        removed_overlays: &'a mut Vec<Vec<u8>>,
    ) -> impl DoubleEndedIterator<Item = &OverlayFrame> + 'a
    where
        'b: 'a,
    {
        self.active_overlay_ids(removed_overlays)
            .into_iter()
            .map(|id| self.get_overlay(id))
    }

    pub fn active_overlay_names(&self, removed_overlays: &mut Vec<Vec<u8>>) -> Vec<&Vec<u8>> {
        self.active_overlay_ids(removed_overlays)
            .iter()
            .map(|id| self.get_overlay_name(*id))
            .collect()
    }

    pub fn get_overlay_name(&self, overlay_id: OverlayId) -> &Vec<u8> {
        &self
            .overlays
            .get(overlay_id)
            .expect("internal error: missing overlay")
            .0
    }

    pub fn get_overlay(&self, overlay_id: OverlayId) -> &OverlayFrame {
        &self
            .overlays
            .get(overlay_id)
            .expect("internal error: missing overlay")
            .1
    }

    pub fn get_overlay_mut(&mut self, overlay_id: OverlayId) -> &mut OverlayFrame {
        &mut self
            .overlays
            .get_mut(overlay_id)
            .expect("internal error: missing overlay")
            .1
    }

    pub fn find_overlay(&self, name: &[u8]) -> Option<OverlayId> {
        self.overlays.iter().position(|(n, _)| n == name)
    }

    pub fn find_active_overlay(&self, name: &[u8]) -> Option<OverlayId> {
        self.overlays
            .iter()
            .position(|(n, _)| n == name)
            .and_then(|id| {
                if self.active_overlays.contains(&id) {
                    Some(id)
                } else {
                    None
                }
            })
    }
}

#[derive(Debug, Clone)]
pub struct OverlayFrame {
    pub vars: HashMap<Vec<u8>, VarId>,
    pub constants: HashMap<VarId, Value>,
    pub predecls: HashMap<Vec<u8>, DeclId>, // temporary storage for predeclarations
    pub decls: HashMap<(Vec<u8>, Type), DeclId>,
    pub modules: HashMap<Vec<u8>, ModuleId>,
    pub visibility: Visibility,
    pub origin: ModuleId, // The original module the overlay was created from
    pub prefixed: bool,   // Whether the overlay has definitions prefixed with its name
}

impl OverlayFrame {
    pub fn from_origin(origin: ModuleId, prefixed: bool) -> Self {
        Self {
            vars: HashMap::new(),
            constants: HashMap::new(),
            predecls: HashMap::new(),
            decls: HashMap::new(),
            modules: HashMap::new(),
            visibility: Visibility::new(),
            origin,
            prefixed,
        }
    }

    pub fn insert_decl(&mut self, name: Vec<u8>, input: Type, decl_id: DeclId) -> Option<DeclId> {
        self.decls.insert((name, input), decl_id)
    }

    pub fn get_decl(&self, name: &[u8], input: &Type) -> Option<DeclId> {
        if let Some(decl) = self.decls.get(&(name, input) as &dyn DeclKey) {
            Some(*decl)
        } else {
            self.decls.get(&(name, &Type::Any) as &dyn DeclKey).cloned()
        }
    }
}

trait DeclKey {
    fn name(&self) -> &[u8];
    fn input(&self) -> &Type;
}

impl Hash for dyn DeclKey + '_ {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.name().hash(state);
        self.input().hash(state);
    }
}

impl PartialEq for dyn DeclKey + '_ {
    fn eq(&self, other: &Self) -> bool {
        self.name() == other.name() && self.input() == other.input()
    }
}

impl Eq for dyn DeclKey + '_ {}

impl<'a> DeclKey for (&'a [u8], &Type) {
    fn name(&self) -> &[u8] {
        self.0
    }

    fn input(&self) -> &Type {
        self.1
    }
}

impl DeclKey for (Vec<u8>, Type) {
    fn name(&self) -> &[u8] {
        &self.0
    }

    fn input(&self) -> &Type {
        &self.1
    }
}

impl<'a> Borrow<dyn DeclKey + 'a> for (Vec<u8>, Type) {
    fn borrow(&self) -> &(dyn DeclKey + 'a) {
        self
    }
}

impl Default for Visibility {
    fn default() -> Self {
        Self::new()
    }
}

impl Default for ScopeFrame {
    fn default() -> Self {
        Self::new()
    }
}
