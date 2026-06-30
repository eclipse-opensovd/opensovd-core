// SPDX-FileCopyrightText: Copyright (c) 2026 Contributors to the Eclipse Foundation
// SPDX-License-Identifier: Apache-2.0

mod cli;

use std::fmt::Write;
use std::process::ExitCode;

use clap::Parser;
use opensovd_client::Client;
use rmcp::{
    RoleServer, ServerHandler, ServiceExt,
    handler::server::{router::prompt::PromptRouter, tool::ToolRouter},
    model::{
        CallToolResult, ErrorData as McpError, GetPromptResult, Implementation,
        ListResourcesResult, PaginatedRequestParams, PromptMessage, ReadResourceRequestParams,
        ReadResourceResult, Resource, ResourceContents, Role, ServerCapabilities, ServerInfo,
    },
    prompt, prompt_handler, prompt_router,
    service::RequestContext,
    tool, tool_handler, tool_router,
};

const TARGET: &str = "srv";

const TOPOLOGY_URI: &str = "sovd://topology";

#[derive(Clone)]
struct McpServer {
    tool_router: ToolRouter<Self>,
    prompt_router: PromptRouter<Self>,
    client: Client,
}

#[tool_router]
impl McpServer {
    #[tool(description = "List all SOVD components")]
    async fn list_components(&self) -> Result<CallToolResult, McpError> {
        let response = self
            .client
            .list_components()
            .schema(true)
            .send()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let value = serde_json::to_value(&response)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(description = "List all SOVD areas")]
    async fn list_areas(&self) -> Result<CallToolResult, McpError> {
        let response = self
            .client
            .list_areas()
            .schema(true)
            .send()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let value = serde_json::to_value(&response)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::structured(value))
    }

    #[tool(description = "List all SOVD apps")]
    async fn list_apps(&self) -> Result<CallToolResult, McpError> {
        let response = self
            .client
            .list_apps()
            .schema(true)
            .send()
            .await
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        let value = serde_json::to_value(&response)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::structured(value))
    }
}

impl McpServer {
    fn new(client: Client) -> Self {
        Self {
            tool_router: Self::tool_router(),
            prompt_router: Self::prompt_router(),
            client,
        }
    }
}

#[prompt_router]
impl McpServer {
    #[prompt(
        name = "explore-topology",
        description = "Explore the vehicle diagnostic topology by listing components, areas, and apps."
    )]
    async fn explore_topology(&self) -> GetPromptResult {
        GetPromptResult::new(vec![PromptMessage::new_text(
            Role::User,
            "Read the sovd://topology resource, then:\n\
                 1. List components\n\
                 2. List areas\n\
                 3. List apps and their hosting relationships\n\
                 4. Summarize the vehicle's diagnostic topology",
        )])
        .with_description("Explore the vehicle diagnostic topology exposed by the SOVD server.")
    }
}

#[tool_handler(router = self.tool_router)]
#[prompt_handler(router = self.prompt_router)]
impl ServerHandler for McpServer {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_resources()
            .enable_prompts()
            .build();
        let server_info = Implementation::new("opensovd-mcp", env!("CARGO_PKG_VERSION"))
            .with_description(env!("CARGO_PKG_DESCRIPTION"));
        ServerInfo::new(capabilities)
            .with_server_info(server_info)
            .with_instructions(
                "OpenSOVD MCP server for vehicle diagnostics. \
                 Base URI: /sovd/v1. \
                 Entity hierarchy: Areas > Components > Apps > Functions. \
                 Each entity may expose: data, faults, operations, configurations, \
                 bulk-data, locks, and modes. \
                 Use the topology resource to explore the vehicle.",
            )
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, McpError> {
        let resource = Resource::new(TOPOLOGY_URI, "Vehicle Topology")
            .with_description("Snapshot of the SOVD entity hierarchy: components, areas, and apps.")
            .with_mime_type("text/plain");
        Ok(ListResourcesResult {
            resources: vec![resource],
            next_cursor: None,
            meta: None,
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, McpError> {
        if request.uri != TOPOLOGY_URI {
            return Err(McpError::resource_not_found(
                format!("unknown resource: {}", request.uri),
                None,
            ));
        }

        let (components, areas, apps) = tokio::try_join!(
            async {
                self.client
                    .list_components()
                    .send()
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))
            },
            async {
                self.client
                    .list_areas()
                    .send()
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))
            },
            async {
                self.client
                    .list_apps()
                    .send()
                    .await
                    .map_err(|e| McpError::internal_error(e.to_string(), None))
            },
        )?;

        let mut text = String::new();

        let _ = writeln!(text, "# Vehicle Topology\n");

        let _ = writeln!(
            text,
            "## Components ({count})\n",
            count = components.data.items.len()
        );
        for component in &components.data.items {
            let _ = writeln!(
                text,
                "- {name} (id: {id})",
                name = component.name,
                id = component.id
            );
        }

        let _ = writeln!(
            text,
            "\n## Areas ({count})\n",
            count = areas.data.items.len()
        );
        for area in &areas.data.items {
            let _ = writeln!(text, "- {name} (id: {id})", name = area.name, id = area.id);
        }

        let _ = writeln!(text, "\n## Apps ({count})\n", count = apps.data.items.len());
        for app in &apps.data.items {
            let _ = writeln!(text, "- {name} (id: {id})", name = app.name, id = app.id);
        }

        Ok(ReadResourceResult::new(vec![ResourceContents::text(
            text,
            TOPOLOGY_URI,
        )]))
    }
}

#[tokio::main(flavor = "current_thread")]
#[allow(clippy::print_stderr)]
async fn main() -> ExitCode {
    let cli = cli::Cli::parse();

    if let Err(e) = libcli::init_tracing("info", Some(&cli.log)) {
        eprintln!("Failed to initialize tracing: {e}");
        return ExitCode::FAILURE;
    }

    if let Err(e) = serve(&cli.url).await {
        eprintln!("Error: {e:?}");
        return ExitCode::FAILURE;
    }

    ExitCode::SUCCESS
}

async fn serve(url: &str) -> anyhow::Result<()> {
    tracing::info!(
        target: TARGET,
        version = %env!("CARGO_PKG_VERSION"),
        sha1 = %env!("COMMIT_SHA"),
        build_date = %env!("BUILD_DATE"),
        "{}", cli::ABOUT
    );
    let client = Client::connect(url)?;
    let service = McpServer::new(client)
        .serve(rmcp::transport::stdio())
        .await?;

    let ct = service.cancellation_token();
    tokio::select! {
        result = service.waiting() => {
            tracing::info!(target: TARGET, ?result, "Service stopped");
        }
        () = libcli::shutdown_signal() => {
            ct.cancel();
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use opensovd_models::discovery::EntityReference;
    use rmcp::model::{CallToolRequestParams, GetPromptRequestParams};
    use rmcp::service::{RoleClient, RunningService};

    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error + Send + Sync>>;

    async fn setup(client: Client) -> RunningService<RoleClient, ()> {
        let (server_transport, client_transport) = tokio::io::duplex(4096);

        tokio::spawn(async move {
            let result = McpServer::new(client).serve(server_transport).await;
            if let Ok(server) = result {
                let _ = server.waiting().await;
            }
        });

        ().serve(client_transport)
            .await
            .expect("client failed to connect")
    }

    fn mock_client(connector: mock_http_connector::Connector) -> Client {
        Client::builder()
            .base_uri("http://localhost/sovd/v1")
            .expect("valid URI")
            .connector(connector)
            .build()
            .expect("valid test client")
    }

    fn entity(collection: &str, id: &str, name: &str) -> EntityReference {
        EntityReference {
            id: id.into(),
            name: name.into(),
            translation_id: None,
            href: format!("/sovd/v1/{collection}/{id}").into(),
            tags: None,
        }
    }

    #[tokio::test]
    async fn list_resources_includes_topology() -> TestResult {
        let connector = mock_http_connector::Connector::builder().build();
        let client = setup(mock_client(connector)).await;

        let resources = client.list_resources(Option::default()).await?;

        assert_eq!(resources.resources.len(), 1);
        assert_eq!(resources.resources[0].uri, TOPOLOGY_URI);
        assert_eq!(resources.resources[0].name, "Vehicle Topology");

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn read_topology_resource() -> TestResult {
        let components = serde_json::to_string(&opensovd_models::Items {
            items: vec![entity("components", "ecu1", "Engine ECU")],
        })?;
        let areas = serde_json::to_string(&opensovd_models::Items {
            items: vec![entity("areas", "powertrain", "Powertrain")],
        })?;
        let apps =
            serde_json::to_string(&opensovd_models::Items::<EntityReference> { items: vec![] })?;

        let mut builder = mock_http_connector::Connector::builder();
        builder
            .expect()
            .with_uri("http://localhost/sovd/v1/components")
            .returning(components)?;
        builder
            .expect()
            .with_uri("http://localhost/sovd/v1/areas")
            .returning(areas)?;
        builder
            .expect()
            .with_uri("http://localhost/sovd/v1/apps")
            .returning(apps)?;

        let client = setup(mock_client(builder.build())).await;

        let result = client
            .read_resource(ReadResourceRequestParams::new(TOPOLOGY_URI))
            .await?;

        let text = match &result.contents[0] {
            ResourceContents::TextResourceContents { text, .. } => text.as_str(),
            _ => panic!("expected text resource contents"),
        };

        assert!(text.contains("Engine ECU"), "expected component name");
        assert!(text.contains("ecu1"), "expected component id");
        assert!(text.contains("Powertrain"), "expected area name");
        assert!(text.contains("## Apps (0)"), "expected empty apps section");

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn list_prompts_includes_explore_topology() -> TestResult {
        let connector = mock_http_connector::Connector::builder().build();
        let client = setup(mock_client(connector)).await;

        let prompts = client.list_prompts(Option::default()).await?;

        let names: Vec<&str> = prompts.prompts.iter().map(|p| p.name.as_ref()).collect();
        assert!(
            names.contains(&"explore-topology"),
            "expected 'explore-topology' in {names:?}"
        );

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn get_explore_topology_prompt() -> TestResult {
        let connector = mock_http_connector::Connector::builder().build();
        let client = setup(mock_client(connector)).await;

        let result = client
            .get_prompt(GetPromptRequestParams::new("explore-topology"))
            .await?;

        assert!(!result.messages.is_empty(), "expected at least one message");
        let msg = &result.messages[0];
        assert_eq!(msg.role, Role::User);
        match &msg.content {
            rmcp::model::ContentBlock::Text(c) => {
                assert!(
                    c.text.contains("topology"),
                    "expected prompt to mention topology"
                );
            }
            _ => panic!("expected text content"),
        }

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn list_components_returns_json() -> TestResult {
        let components = serde_json::to_string(&opensovd_models::Items {
            items: vec![entity("components", "ecu1", "Engine ECU")],
        })?;

        let mut builder = mock_http_connector::Connector::builder();
        builder
            .expect()
            .with_uri("http://localhost/sovd/v1/components?include-schema=true")
            .returning(components)?;

        let client = setup(mock_client(builder.build())).await;

        let result = client
            .call_tool(CallToolRequestParams::new("list_components"))
            .await?;

        let structured = result
            .structured_content
            .expect("expected structured content");
        let text = serde_json::to_string(&structured)?;
        assert!(text.contains("Engine ECU"), "expected component name");
        assert!(text.contains("ecu1"), "expected component id");

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn list_areas_returns_json() -> TestResult {
        let areas = serde_json::to_string(&opensovd_models::Items {
            items: vec![entity("areas", "powertrain", "Powertrain")],
        })?;

        let mut builder = mock_http_connector::Connector::builder();
        builder
            .expect()
            .with_uri("http://localhost/sovd/v1/areas?include-schema=true")
            .returning(areas)?;

        let client = setup(mock_client(builder.build())).await;

        let result = client
            .call_tool(CallToolRequestParams::new("list_areas"))
            .await?;

        let structured = result
            .structured_content
            .expect("expected structured content");
        let text = serde_json::to_string(&structured)?;
        assert!(text.contains("Powertrain"), "expected area name");
        assert!(text.contains("powertrain"), "expected area id");

        client.cancel().await?;
        Ok(())
    }

    #[tokio::test]
    async fn list_apps_returns_json() -> TestResult {
        let apps = serde_json::to_string(&opensovd_models::Items {
            items: vec![entity("apps", "diag_app", "Diagnostic App")],
        })?;

        let mut builder = mock_http_connector::Connector::builder();
        builder
            .expect()
            .with_uri("http://localhost/sovd/v1/apps?include-schema=true")
            .returning(apps)?;

        let client = setup(mock_client(builder.build())).await;

        let result = client
            .call_tool(CallToolRequestParams::new("list_apps"))
            .await?;

        let structured = result
            .structured_content
            .expect("expected structured content");
        let text = serde_json::to_string(&structured)?;
        assert!(text.contains("Diagnostic App"), "expected app name");
        assert!(text.contains("diag_app"), "expected app id");

        client.cancel().await?;
        Ok(())
    }
}
