// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

//! Entity paths with their ids and kinds.

use std::fmt;
use std::sync::{Arc, LazyLock};

/// `id` as a path segment, unless it is empty, `.` or `..`, which URL
/// normalization removes even when percent-encoded.
fn checked(id: &str) -> Result<Arc<str>, InvalidPath> {
    match id {
        "" | "." | ".." => Err(InvalidPath::Id(id.to_owned())),
        id => Ok(id.into()),
    }
}

/// The kind of an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum EntityKind {
    Area,
    Component,
    App,
    Function,
    /// The API root. Its only path is [`EntityPath::server`].
    Server,
}

impl EntityKind {
    /// The top-level collection, such as `areas`. Empty for the server,
    /// which is in no collection.
    #[must_use]
    pub(crate) const fn collection(self) -> &'static str {
        match self {
            Self::Area => "areas",
            Self::Component => "components",
            Self::App => "apps",
            Self::Function => "functions",
            Self::Server => "",
        }
    }

    /// The collection of nested entities, or `None` if the kind does not nest.
    #[must_use]
    pub(crate) const fn child_collection(self) -> Option<&'static str> {
        match self {
            Self::Area => Some("subareas"),
            Self::Component => Some("subcomponents"),
            Self::App | Self::Function | Self::Server => None,
        }
    }
}

/// An entity path: a kind and the ids from the top level down.
/// Nesting stays within one kind, so `components/Cabin/subcomponents/Display`
/// is a component with two ids. The server path has no ids. Cloning is a
/// reference-count increment.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EntityPath {
    kind: EntityKind,
    ids: Arc<[Arc<str>]>,
}

/// The path of the server entity, which every [`EntityPath::server`] shares.
static SERVER: LazyLock<EntityPath> = LazyLock::new(|| EntityPath {
    kind: EntityKind::Server,
    ids: Arc::from([]),
});

impl EntityPath {
    /// The server entity, the API root, which has no segments. Every call
    /// shares one path, so none allocates.
    #[must_use]
    pub fn server() -> Self {
        SERVER.clone()
    }

    /// `areas/{id}`.
    ///
    /// # Errors
    ///
    /// [`InvalidPath::Id`] if `id` is empty, `.` or `..`.
    pub fn area(id: impl AsRef<str>) -> Result<AreaPath, InvalidPath> {
        Self::new(EntityKind::Area, id).map(AreaPath)
    }

    /// `components/{id}`.
    ///
    /// # Errors
    ///
    /// [`InvalidPath::Id`] if `id` is empty, `.` or `..`.
    pub fn component(id: impl AsRef<str>) -> Result<ComponentPath, InvalidPath> {
        Self::new(EntityKind::Component, id).map(ComponentPath)
    }

    /// `apps/{id}`.
    ///
    /// # Errors
    ///
    /// [`InvalidPath::Id`] if `id` is empty, `.` or `..`.
    pub fn app(id: impl AsRef<str>) -> Result<AppPath, InvalidPath> {
        Self::new(EntityKind::App, id).map(AppPath)
    }

    /// `functions/{id}`.
    ///
    /// # Errors
    ///
    /// [`InvalidPath::Id`] if `id` is empty, `.` or `..`.
    pub fn function(id: impl AsRef<str>) -> Result<FunctionPath, InvalidPath> {
        Self::new(EntityKind::Function, id).map(FunctionPath)
    }

    fn new(kind: EntityKind, id: impl AsRef<str>) -> Result<Self, InvalidPath> {
        Ok(Self {
            kind,
            ids: Arc::from([checked(id.as_ref())?]),
        })
    }

    /// The path one level below. Entities nest one level only.
    fn join(&self, id: impl AsRef<str>) -> Result<Self, InvalidPath> {
        if self.ids.len() > 1 {
            return Err(InvalidPath::Nested(self.clone()));
        }
        let id = checked(id.as_ref())?;
        Ok(Self {
            kind: self.kind,
            ids: self
                .ids
                .iter()
                .cloned()
                .chain(std::iter::once(id))
                .collect(),
        })
    }

    #[must_use]
    pub const fn kind(&self) -> EntityKind {
        self.kind
    }

    /// The last id, empty for the server.
    #[must_use]
    pub fn id(&self) -> &str {
        self.ids.last().map_or("", AsRef::as_ref)
    }

    /// The collection and id of each level from the top down, such as
    /// `components` and `Cabin`, then `subcomponents` and `Display`. The ids
    /// are not percent-encoded.
    pub fn segments(&self) -> impl Iterator<Item = (&'static str, &str)> {
        let top = self.kind.collection();
        let nested = self.kind.child_collection().unwrap_or(top);
        self.ids
            .iter()
            .enumerate()
            .map(move |(i, id)| (if i == 0 { top } else { nested }, &**id))
    }

    /// The enclosing entity, or `None` at the top level.
    #[must_use]
    pub fn parent(&self) -> Option<Self> {
        let (_, ids) = self.ids.split_last()?;
        (!ids.is_empty()).then(|| Self {
            kind: self.kind,
            ids: Arc::from(ids),
        })
    }
}

macro_rules! typed_path {
    ($(#[$doc:meta])* $ty:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $ty(pub(super) EntityPath);

        impl From<$ty> for EntityPath {
            fn from(path: $ty) -> Self {
                path.0
            }
        }
    };
}

typed_path!(
    /// The path of an area or a subarea.
    AreaPath
);
typed_path!(
    /// The path of a component or a subcomponent.
    ComponentPath
);
typed_path!(
    /// The path of an app.
    AppPath
);
typed_path!(
    /// The path of a function.
    FunctionPath
);

impl AreaPath {
    /// The path of a subarea.
    ///
    /// # Errors
    ///
    /// [`InvalidPath::Id`] if `id` is empty, `.` or `..`, and
    /// [`InvalidPath::Nested`] below a subarea, as entities nest one level only.
    pub fn child(&self, id: impl AsRef<str>) -> Result<Self, InvalidPath> {
        self.0.join(id).map(Self)
    }
}

impl ComponentPath {
    /// The path of a subcomponent.
    ///
    /// # Errors
    ///
    /// [`InvalidPath::Id`] if `id` is empty, `.` or `..`, and
    /// [`InvalidPath::Nested`] below a subcomponent, as entities nest one level only.
    pub fn child(&self, id: impl AsRef<str>) -> Result<Self, InvalidPath> {
        self.0.join(id).map(Self)
    }
}

/// Why a path cannot be built.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InvalidPath {
    /// An id is empty, `.` or `..`.
    Id(String),
    /// A sub-entity below this nested entity: entities nest one level only.
    Nested(EntityPath),
}

impl fmt::Display for InvalidPath {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Id(id) => write!(
                f,
                "invalid entity id {id:?}: must not be empty, `.` or `..`"
            ),
            Self::Nested(path) => write!(
                f,
                "the {:?} {:?} is nested already, and entities nest one level only",
                path.kind, path.ids
            ),
        }
    }
}

impl std::error::Error for InvalidPath {}

#[cfg(test)]
mod tests {
    use super::*;

    fn comfort() -> AreaPath {
        EntityPath::area("Comfort").unwrap()
    }

    fn media() -> AreaPath {
        comfort().child("Media").unwrap()
    }

    fn cabin() -> ComponentPath {
        EntityPath::component("Cabin").unwrap()
    }

    fn display() -> ComponentPath {
        cabin().child("Display").unwrap()
    }

    fn radio() -> AppPath {
        EntityPath::app("Radio").unwrap()
    }

    fn self_test() -> FunctionPath {
        EntityPath::function("SelfTest").unwrap()
    }

    fn ids(path: &EntityPath) -> Vec<&str> {
        path.ids.iter().map(AsRef::as_ref).collect()
    }

    fn segments(path: &EntityPath) -> Vec<(&'static str, &str)> {
        path.segments().collect()
    }

    #[test]
    fn ids_must_not_be_empty_dot_or_dot_dot() {
        for bad in ["", ".", ".."] {
            assert_eq!(checked(bad), Err(InvalidPath::Id(bad.to_owned())));
        }
        for good in ["...", ".hidden", "Front Left", "a/b", "Ölstand"] {
            assert_eq!(checked(good).as_deref(), Ok(good));
        }
    }

    #[test]
    fn kinds_name_their_collections() {
        let table = [
            (EntityKind::Area, "areas", Some("subareas")),
            (EntityKind::Component, "components", Some("subcomponents")),
            (EntityKind::App, "apps", None),
            (EntityKind::Function, "functions", None),
            (EntityKind::Server, "", None),
        ];
        for (kind, collection, child_collection) in table {
            assert_eq!(kind.collection(), collection, "{kind:?}");
            assert_eq!(kind.child_collection(), child_collection, "{kind:?}");
        }
    }

    #[test]
    fn top_level_paths_have_one_id() {
        let cases: [(EntityPath, EntityKind, &str); 4] = [
            (comfort().into(), EntityKind::Area, "areas"),
            (cabin().into(), EntityKind::Component, "components"),
            (radio().into(), EntityKind::App, "apps"),
            (self_test().into(), EntityKind::Function, "functions"),
        ];
        for (path, kind, collection) in cases {
            assert_eq!(path.kind(), kind);
            assert_eq!(ids(&path), [path.id()]);
            assert_eq!(path.parent(), None);
            assert_eq!(segments(&path), [(collection, path.id())]);
        }
        assert_ne!(
            EntityPath::from(radio()),
            EntityPath::from(EntityPath::function("Radio").unwrap()),
            "the kind is part of the path"
        );
        for bad in ["", ".", ".."] {
            let error = Some(InvalidPath::Id(bad.to_owned()));
            assert_eq!(EntityPath::area(bad).err(), error);
            assert_eq!(EntityPath::component(bad).err(), error);
            assert_eq!(EntityPath::app(bad).err(), error);
            assert_eq!(EntityPath::function(bad).err(), error);
        }
    }

    #[test]
    fn the_server_path_has_no_ids_and_is_shared() {
        let server = EntityPath::server();
        assert_eq!(server.kind(), EntityKind::Server);
        assert_eq!(ids(&server), Vec::<&str>::new());
        assert_eq!(server.id(), "");
        assert_eq!(server.parent(), None);
        assert_eq!(segments(&server), []);
        assert!(Arc::ptr_eq(&server.ids, &EntityPath::server().ids));
    }

    #[test]
    fn areas_and_components_nest_one_level() {
        let media = media();
        let path = EntityPath::from(media.clone());
        assert_eq!(path.kind(), EntityKind::Area);
        assert_eq!(ids(&path), ["Comfort", "Media"]);
        assert_eq!(path.id(), "Media");
        assert_eq!(path.parent(), Some(comfort().into()));
        assert_eq!(media.child("Radio"), Err(InvalidPath::Nested(path)));

        let display = display();
        let path = EntityPath::from(display.clone());
        assert_eq!(path.kind(), EntityKind::Component);
        assert_eq!(ids(&path), ["Cabin", "Display"]);
        assert_eq!(path.id(), "Display");
        assert_eq!(path.parent(), Some(cabin().into()));
        assert_eq!(display.child("Panel"), Err(InvalidPath::Nested(path)));

        let twin = cabin().child("Cabin").unwrap();
        assert_ne!(twin, cabin(), "a subcomponent may reuse its parent's id");
        for bad in ["", ".", ".."] {
            assert_eq!(comfort().child(bad), Err(InvalidPath::Id(bad.to_owned())));
            assert_eq!(cabin().child(bad), Err(InvalidPath::Id(bad.to_owned())));
        }
    }

    #[test]
    fn segments_name_the_collection_and_id_of_each_level() {
        let front_left: EntityPath = cabin().child("Front Left/1").unwrap().into();
        assert_eq!(
            segments(&media().into()),
            [("areas", "Comfort"), ("subareas", "Media")]
        );
        assert_eq!(
            segments(&display().into()),
            [("components", "Cabin"), ("subcomponents", "Display")]
        );
        assert_eq!(
            segments(&front_left),
            [("components", "Cabin"), ("subcomponents", "Front Left/1")]
        );
    }

    #[test]
    fn errors_explain_themselves() {
        let cases = [
            (
                InvalidPath::Id("..".to_owned()),
                "invalid entity id \"..\": must not be empty, `.` or `..`",
            ),
            (
                InvalidPath::Nested(display().into()),
                "the Component [\"Cabin\", \"Display\"] is nested already, and entities nest \
                 one level only",
            ),
        ];
        for (error, message) in cases {
            assert_eq!(error.to_string(), message);
        }
    }
}
