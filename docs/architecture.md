# Architecture

## Class Diagram

```mermaid
classDiagram
    direction TB

    class Server {
        -listener: Listener
        -authenticator: A
        -authorizer: Z
        -topology: Topology
        -layer: L
        +builder() ServerBuilder
        +serve() Result
    }

    class Topology {
        -inner: Arc~RwLock~TopologyState~~
        -events: broadcast::Sender~TopologyEvent~
        +new() Topology
        +with_event_capacity(capacity) Topology
        +read() TopologyReadGuard
        +write() TopologyWriteGuard
        +subscribe() Receiver~TopologyEvent~
    }

    class TopologyWriteGuard {
        +add_component(component)
        +add_app(app)
        +add_area(area)
        +remove_component(id)
        +remove_app(id)
        +remove_area(id)
    }

    class TopologyReadGuard {
        +get_component(entity_ref) Result~Component~
        +get_app(entity_ref) Result~App~
        +get_area(entity_ref) Result~Area~
        +components() Values~Component~
        +apps() Values~App~
        +areas() Values~Area~
        +apps_of_component(component_id) Vec~App~
        +components_of_area(area_id) Vec~Component~
        +apps_of_area(area_id) Vec~App~
        +component_of_app(app_id) Result~Option~Component~~
        +area_of_component(component_id) Result~Option~Area~~
        +area_of_app(app_id) Result~Option~Area~~
    }

    class EntityKind {
        <<enumeration>>
        Component
        App
        Area
    }

    class EntityRef {
        -kind: EntityKind
        -id: String
        +component(id) EntityRef
        +app(id) EntityRef
        +area(id) EntityRef
        +kind() EntityKind
        +id() str
    }

    class Component {
        -entity_ref: EntityRef
        -name: String
        -area_id: Option~String~
        -metadata: HashMap~String, String~
        -tags: Vec~String~
        -translation_id: Option~String~
        -data_provider: Option~Arc~DataProvider~~
    }

    class App {
        -entity_ref: EntityRef
        -name: String
        -is_located_on: String
        -area_id: Option~String~
        -metadata: HashMap~String, String~
        -tags: Vec~String~
        -translation_id: Option~String~
        -data_provider: Option~Arc~DataProvider~~
    }

    class Area {
        -entity_ref: EntityRef
        -name: String
        -metadata: HashMap~String, String~
        -tags: Vec~String~
        -translation_id: Option~String~
        -data_provider: Option~Arc~DataProvider~~
    }

    class EntityCollection {
        +components: Vec~Component~
        +apps: Vec~App~
        +areas: Vec~Area~
        +entity_refs() Vec~EntityRef~
    }

    class Authenticator {
        <<trait>>
        +type Identity
        +authenticate(parts) Future~Result~Identity, AuthError~~
    }

    class Authorizer~I~ {
        <<trait>>
        +authorize(identity, parts) Future~Result~(), AuthError~~
    }

    class NoAuth {
        Identity = ()
    }

    class AllowAll

    class DataProvider {
        <<trait>>
        +list(filter) Result~Vec~ValueMetadata~~
        +read(data_id, include_schema) Result~DataValue~
        +write(data_id, value) Result
        +categories() Result~Vec~CategoryInfo~~
        +groups(category_filter) Result~Vec~GroupInfo~~
        +tags() Result~Vec~TagInfo~~
    }

    Server --> Topology : uses
    Server --> Authenticator : uses
    Server --> Authorizer : uses
    Topology --> TopologyReadGuard : produces
    Topology --> TopologyWriteGuard : produces
    TopologyReadGuard --> Component : queries
    TopologyReadGuard --> App : queries
    TopologyReadGuard --> Area : queries
    Component --> EntityRef : has
    App --> EntityRef : has
    Area --> EntityRef : has
    Component --> DataProvider : optional
    App --> DataProvider : optional
    Area --> DataProvider : optional
    EntityRef --> EntityKind : has
    NoAuth ..|> Authenticator : implements
    AllowAll ..|> Authorizer : implements
    Topology --> EntityCollection : accepts
```

## Main Components

### Server

The main entry point for the OpenSOVD application. Built using a builder pattern that allows configuring authentication, authorization, topology, and custom Tower layers. Supports both TCP and Unix socket listeners. On `serve()`, it starts the Axum HTTP server with graceful shutdown.

**Crate:** `opensovd-server`

### Topology

A thread-safe registry for components, apps, and areas. Wraps an `Arc<RwLock<TopologyState>>` where the state holds three `IndexMap` collections (one per entity kind) plus relationship indexes (`apps_by_component`, `components_by_area`, `apps_by_area`). Reading is done through `read()` which returns a `TopologyReadGuard` holding a single read lock for consistent multi-query access. Mutations are done through `write()` which returns a `TopologyWriteGuard` holding a single write lock; `add_*` overwrites an existing entry with the same ID and `remove_*` silently skips missing IDs. `TopologyEvent::Added` / `TopologyEvent::Removed` events are batched on the guard and flushed via a `broadcast` channel when the guard is dropped. Callers can `subscribe()` to receive these events.

**Crate:** `opensovd-core`

### TopologyReadGuard

A read guard over the topology state. Holds a `RwLockReadGuard` for its lifetime, ensuring a consistent snapshot across multiple queries. Provides `get_component()`, `get_app()`, `get_area()` for single-entity lookup, `components()`, `apps()`, `areas()` for listing, and relationship queries: `apps_of_component()`, `components_of_area()`, `apps_of_area()`, `component_of_app()`, `area_of_component()`, `area_of_app()`.

**Crate:** `opensovd-core`

### TopologyWriteGuard

A write guard over the topology state. Holds a `RwLockWriteGuard` for its lifetime so a batch of mutations applies atomically. Provides `add_component()`, `add_app()`, `add_area()` (each overwrites an existing entry with the same ID) and `remove_component()`, `remove_app()`, `remove_area()` (each silently skips missing IDs). Topology events are accumulated in the guard and flushed to the broadcast channel when the guard is dropped.

**Crate:** `opensovd-core`

### EntityRef / EntityKind

`EntityKind` is an enum with variants `Component`, `App`, and `Area`. `EntityRef` is a lightweight reference containing a `kind` and `id`. Factory methods `EntityRef::component()`, `EntityRef::app()`, and `EntityRef::area()` create refs. All entity structs hold an `EntityRef` and expose it via `entity_ref()`. `EntityCollection` groups `Vec<Component>`, `Vec<App>`, and `Vec<Area>` for batch topology operations.

**Crate:** `opensovd-core`

### Component

A SOVD component entity. Fields: `entity_ref`, `name`, `area_id` (optional link to an area), `metadata` (key-value pairs), `tags`, `translation_id`, and an optional `Arc<dyn DataProvider>`. Constructed via `Component::new(id, name)` with builder methods (`with_area_id`, `with_tags`, `with_metadata`, `with_data_provider`, etc.).

**Crate:** `opensovd-core`

### App

A SOVD app entity representing software running on a component. Fields: `entity_ref`, `name`, `is_located_on` (component ID, the "is-located-on" relationship), `area_id` (optional), `metadata`, `tags`, `translation_id`, and an optional `Arc<dyn DataProvider>`. Constructed via `App::new(id, name, is_located_on)` with builder methods.

**Crate:** `opensovd-core`

### Area

A SOVD area entity representing a logical view of vehicle architecture (e.g., domain or zone). Fields: `entity_ref`, `name`, `metadata`, `tags`, `translation_id`, and an optional `Arc<dyn DataProvider>`. Constructed via `Area::new(id, name)` with builder methods.

**Crate:** `opensovd-core`

### DataProvider (trait)

A pluggable data backend. Each entity holds its own provider instance, so methods operate on the provider directly without an entity reference:

- `list(filter: DataFilter) → Vec<ValueMetadata>` — list data items, filtered by categories/groups/tags
- `read(data_id, include_schema) → DataValue` — read a single data value
- `write(data_id, value) → ()` — write a data value

Default implementations derive from `list()`:

- `categories() → Vec<CategoryInfo>` — unique categories
- `groups(category_filter) → Vec<GroupInfo>` — groups optionally filtered by category
- `tags() → Vec<TagInfo>` — unique tags

**Crate:** `opensovd-core`

### Authenticator (trait)

Extracts and validates credentials from incoming requests. Has an associated type `Identity` representing the authenticated principal. Returns `Ok(Identity)` on success or `Err(AuthError::Unauthenticated)` on failure. `NoAuth` is the no-op implementation (Identity = `()`). Blanket impl for `Option<A>` returns `Option<A::Identity>`, allowing optional authentication.

**Crate:** `opensovd-server`

### Authorizer (trait)

Determines whether an authenticated identity has permission for a request. Generic over the identity type `I`. Takes `(identity, request_parts)` and returns `Ok(())` or `Err(AuthError::Unauthorized)`. Implementation: `AllowAll` permits all requests.

**Crate:** `opensovd-server`

## Request Lifecycle

The following diagram shows how an HTTP request flows through the server: from the middleware stack (authentication and authorization) to the router, which dispatches to entity, data, or version handlers.

```mermaid
sequenceDiagram
    participant C as Client
    participant S as Server
    participant MW as Middleware Stack
    participant R as Router
    participant Top as Topology
    participant DP as DataProvider

    C->>S: HTTP Request
    S->>MW: Forward request
    MW->>MW: UserLayer → AuthN → AuthZ

    alt Middleware rejection
        MW-->>C: 401/403 Error
    end

    MW->>R: Route to handler

    alt Entity Routes (/v1/components, /apps, /areas, /{id}, relationships)
        R->>Top: topology.read()
        Top-->>R: TopologyReadGuard
        R->>R: Query guard (get_component, get_app, get_area, ...)
        R->>R: Build capabilities response
    else Data Routes (/v1/{entity}/{id}/data/...)
        R->>Top: topology.read()
        Top-->>R: TopologyReadGuard
        R->>R: Get entity via guard
        R->>DP: list / read / write / categories / groups / tags
        DP-->>R: Data result
    else Version Route (/version-info)
        R->>R: Return version info
    end

    R-->>MW: Response
    MW-->>C: HTTP Response
```

### Middleware Stack

Every request passes through the middleware stack in this order:

1. **User layers** (CORS, tracing, etc.) — applied outermost via the configurable `Layer` generic.
2. **`AuthenticationLayer`** — calls `Authenticator::authenticate()` to extract an identity from the request. On success, the identity is stored in request extensions. On failure, the request is rejected immediately with **401 (empty body)**.
3. **`AuthorizationLayer`** — retrieves the identity from request extensions (set by AuthN) and calls `Authorizer::authorize()`. On failure, the request is rejected with **403** and a JSON `GenericError` body containing the `insufficient-access-rights` error code.

If both layers pass, the request is forwarded to the matched route handler.

### Entity Routes

`GET /{entity-collection}` (e.g., `/components`, `/apps`, `/areas`) acquires a read lock via `topology.read()` → `TopologyReadGuard`, iterates/filters the matching entities, and returns a JSON array of `EntityReference` objects. `GET /{entity-collection}/{id}` looks up a single entity and returns its capabilities, including relationship hrefs and data provider availability. Returns **404** if the entity is not found.

### Data Routes

`GET /{entity}/{id}/data/{data_id}` acquires a read lock, fetches the entity from the `TopologyReadGuard`, extracts its `DataProvider`, and calls `read()`. `PUT` follows the same lookup but calls `write()` instead. Returns **404** if the entity or data item is not found, **400** if a write is attempted on a read-only item, and **204 No Content** on a successful write.

### Version Route

`GET /version-info` returns the SOVD specification version, base URI, and optional vendor-specific information. This route does not access the topology.
