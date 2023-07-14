use anyhow::{Context, Result};
use next_core::{
    app_structure::{
        get_entrypoints, Entrypoint as AppEntrypoint, EntrypointsVc as AppEntrypointsVc,
        LoaderTreeVc,
    },
    emit_all_assets,
    mode::NextMode,
    next_app::{
        get_app_client_references_chunks, get_app_client_shared_chunks, get_app_page_entry,
        AppEntryVc,
    },
    next_client::{
        get_client_module_options_context, get_client_resolve_options_context,
        get_client_runtime_entries, ClientContextType,
    },
    next_client_reference::{
        ClientReferenceGraphVc, ClientReferenceType, NextEcmascriptClientReferenceTransitionVc,
    },
    next_dynamic::{NextDynamicEntriesVc, NextDynamicTransitionVc},
    next_manifests::{AppBuildManifest, AppPathsManifest, BuildManifest, ClientReferenceManifest},
    next_server::{
        get_server_module_options_context, get_server_resolve_options_context,
        get_server_runtime_entries, ServerContextType,
    },
};
use serde::{Deserialize, Serialize};
use turbo_tasks::{primitives::StringVc, trace::TraceRawVcs, CompletionVc, TryJoinIterExt, Value};
use turbopack_binding::{
    turbo::{
        tasks_env::{CustomProcessEnvVc, ProcessEnvVc},
        tasks_fs::{File, FileSystemPathVc},
    },
    turbopack::{
        core::{
            asset::{Asset, AssetsVc},
            chunk::EvaluatableAssetsVc,
            output::OutputAssetsVc,
            raw_output::RawOutputVc,
            virtual_source::VirtualSourceVc,
        },
        turbopack::{
            module_options::ModuleOptionsContextVc,
            resolve_options_context::ResolveOptionsContextVc,
            transition::{ContextTransitionVc, TransitionsByNameVc},
            ModuleAssetContextVc,
        },
    },
};

use crate::{
    entrypoints::{Entrypoints, EntrypointsVc},
    project::ProjectVc,
    route::{Endpoint, EndpointVc, Route, RouteVc, WrittenEndpoint, WrittenEndpointVc},
};

#[turbo_tasks::value]
pub struct AppProject {
    project: ProjectVc,
    app_dir: FileSystemPathVc,
    mode: NextMode,
}

#[turbo_tasks::value(transparent)]
pub struct OptionAppProject(Option<AppProjectVc>);

impl AppProjectVc {
    fn client_ty(self) -> ClientContextType {
        ClientContextType::App {
            app_dir: self.app_dir(),
        }
    }

    fn rsc_ty(self) -> ServerContextType {
        ServerContextType::AppRSC {
            app_dir: self.app_dir(),
            client_transition: Some(self.client_transition().into()),
            ecmascript_client_reference_transition_name: Some(self.client_transition_name()),
        }
    }

    fn ssr_ty(self) -> ServerContextType {
        ServerContextType::AppSSR {
            app_dir: self.app_dir(),
        }
    }
}

const ECMASCRIPT_CLIENT_TRANSITION_NAME: &str = "next-ecmascript-client-reference";

#[turbo_tasks::value_impl]
impl AppProjectVc {
    #[turbo_tasks::function]
    pub fn new(project: ProjectVc, app_dir: FileSystemPathVc, mode: NextMode) -> Self {
        AppProject {
            project,
            app_dir,
            mode,
        }
        .cell()
    }

    #[turbo_tasks::function]
    async fn project(self) -> Result<ProjectVc> {
        Ok(self.await?.project)
    }

    #[turbo_tasks::function]
    async fn app_dir(self) -> Result<FileSystemPathVc> {
        Ok(self.await?.app_dir)
    }

    #[turbo_tasks::function]
    fn app_entrypoints(self) -> AppEntrypointsVc {
        get_entrypoints(
            self.app_dir(),
            self.project().next_config().page_extensions(),
        )
    }

    #[turbo_tasks::function]
    async fn client_module_options_context(self) -> Result<ModuleOptionsContextVc> {
        let this = self.await?;
        Ok(get_client_module_options_context(
            self.project().project_path(),
            self.project().execution_context(),
            self.project().client_compile_time_info().environment(),
            Value::new(self.client_ty()),
            this.mode,
            self.project().next_config(),
        ))
    }

    #[turbo_tasks::function]
    async fn client_resolve_options_context(self) -> Result<ResolveOptionsContextVc> {
        let this = self.await?;
        Ok(get_client_resolve_options_context(
            self.project().project_path(),
            Value::new(self.client_ty()),
            this.mode,
            self.project().next_config(),
            self.project().execution_context(),
        ))
    }

    #[turbo_tasks::function]
    fn client_transition_name(self) -> StringVc {
        StringVc::cell(ECMASCRIPT_CLIENT_TRANSITION_NAME.to_string())
    }

    #[turbo_tasks::function]
    fn client_transition(self) -> ContextTransitionVc {
        ContextTransitionVc::new(
            self.project().client_compile_time_info(),
            self.client_module_options_context(),
            self.client_resolve_options_context(),
        )
    }

    #[turbo_tasks::function]
    async fn rsc_module_options_context(self) -> Result<ModuleOptionsContextVc> {
        let this = self.await?;
        Ok(get_server_module_options_context(
            self.project().project_path(),
            self.project().execution_context(),
            Value::new(self.rsc_ty()),
            this.mode,
            self.project().next_config(),
        ))
    }

    #[turbo_tasks::function]
    async fn rsc_resolve_options_context(self) -> Result<ResolveOptionsContextVc> {
        let this = self.await?;
        Ok(get_server_resolve_options_context(
            self.project().project_path(),
            Value::new(self.rsc_ty()),
            this.mode,
            self.project().next_config(),
            self.project().execution_context(),
        ))
    }

    #[turbo_tasks::function]
    fn rsc_module_context(self) -> ModuleAssetContextVc {
        let transitions = [
            (
                ECMASCRIPT_CLIENT_TRANSITION_NAME.to_string(),
                NextEcmascriptClientReferenceTransitionVc::new(
                    self.client_transition(),
                    self.ssr_transition(),
                )
                .into(),
            ),
            (
                "next-dynamic".to_string(),
                NextDynamicTransitionVc::new(self.client_transition()).into(),
            ),
        ]
        .into_iter()
        .collect();
        ModuleAssetContextVc::new(
            TransitionsByNameVc::cell(transitions),
            self.project().server_compile_time_info(),
            self.rsc_module_options_context(),
            self.rsc_resolve_options_context(),
        )
    }

    #[turbo_tasks::function]
    fn client_module_context(self) -> ModuleAssetContextVc {
        ModuleAssetContextVc::new(
            TransitionsByNameVc::cell(Default::default()),
            self.project().client_compile_time_info(),
            self.client_module_options_context(),
            self.client_resolve_options_context(),
        )
    }

    #[turbo_tasks::function]
    async fn ssr_module_options_context(self) -> Result<ModuleOptionsContextVc> {
        let this = self.await?;
        Ok(get_server_module_options_context(
            self.project().project_path(),
            self.project().execution_context(),
            Value::new(self.ssr_ty()),
            this.mode,
            self.project().next_config(),
        ))
    }

    #[turbo_tasks::function]
    async fn ssr_resolve_options_context(self) -> Result<ResolveOptionsContextVc> {
        let this = self.await?;
        Ok(get_server_resolve_options_context(
            self.project().project_path(),
            Value::new(self.ssr_ty()),
            this.mode,
            self.project().next_config(),
            self.project().execution_context(),
        ))
    }

    #[turbo_tasks::function]
    fn ssr_transition(self) -> ContextTransitionVc {
        ContextTransitionVc::new(
            self.project().server_compile_time_info(),
            self.ssr_module_options_context(),
            self.ssr_resolve_options_context(),
        )
    }

    #[turbo_tasks::function]
    async fn rsc_runtime_entries(self) -> Result<EvaluatableAssetsVc> {
        let this = self.await?;
        Ok(get_server_runtime_entries(
            self.project().project_path(),
            // TODO(alexkirsz) Should we pass env here or EnvMap::empty, as is done in
            // app_source?
            self.project().env(),
            Value::new(self.rsc_ty()),
            this.mode,
            self.project().next_config(),
        )
        .resolve_entries(self.rsc_module_context().into()))
    }

    #[turbo_tasks::function]
    fn client_env(self) -> ProcessEnvVc {
        CustomProcessEnvVc::new(self.project().env(), self.project().next_config().env())
            .as_process_env()
    }

    #[turbo_tasks::function]
    async fn client_runtime_entries(self) -> Result<EvaluatableAssetsVc> {
        let this = self.await?;
        Ok(get_client_runtime_entries(
            self.project().project_path(),
            self.client_env(),
            Value::new(self.client_ty()),
            this.mode,
            self.project().next_config(),
            self.project().execution_context(),
        )
        .resolve_entries(self.client_module_context().into()))
    }

    #[turbo_tasks::function]
    pub async fn entrypoints(self) -> Result<EntrypointsVc> {
        let app_entrypoints = self.app_entrypoints();
        Ok(Entrypoints {
            routes: app_entrypoints
                .await?
                .iter()
                .map(|(pathname, app_entrypoint)| async {
                    Ok((
                        pathname.clone(),
                        *app_entry_point_to_route(self, *app_entrypoint, pathname.clone()).await?,
                    ))
                })
                .try_join()
                .await?
                .into_iter()
                .collect(),
            middleware: None,
        }
        .into())
    }
}

#[turbo_tasks::function]
pub async fn app_entry_point_to_route(
    app_project: AppProjectVc,
    entrypoint: AppEntrypoint,
    pathname: String,
) -> RouteVc {
    match entrypoint {
        AppEntrypoint::AppPage { loader_tree } => Route::AppPage {
            html_endpoint: AppPageEndpoint {
                ty: AppPageEndpointType::Html,
                loader_tree,
                app_project,
                pathname: pathname.clone(),
            }
            .cell()
            .into(),
            rsc_endpoint: AppPageEndpoint {
                ty: AppPageEndpointType::Rsc,
                loader_tree,
                app_project,
                pathname,
            }
            .cell()
            .into(),
        },
        AppEntrypoint::AppRoute { .. } => Route::AppRoute {
            endpoint: AppRouteEndpoint.cell().into(),
        },
    }
    .cell()
}

#[derive(Copy, Clone, Serialize, Deserialize, PartialEq, Eq, Debug, TraceRawVcs)]
enum AppPageEndpointType {
    Html,
    Rsc,
}

#[turbo_tasks::value]
struct AppPageEndpoint {
    ty: AppPageEndpointType,
    loader_tree: LoaderTreeVc,
    app_project: AppProjectVc,
    pathname: String,
}

#[turbo_tasks::value_impl]
impl AppPageEndpointVc {
    #[turbo_tasks::function]
    async fn app_entry(self) -> Result<AppEntryVc> {
        let this = self.await?;
        Ok(get_app_page_entry(
            this.app_project.rsc_module_context(),
            this.loader_tree,
            this.app_project.app_dir(),
            this.pathname.clone(),
            this.app_project.project().project_path(),
        ))
    }

    #[turbo_tasks::function]
    async fn app_project(self) -> Result<AppProjectVc> {
        let this = self.await?;
        Ok(this.app_project)
    }
}

#[turbo_tasks::value_impl]
impl Endpoint for AppPageEndpoint {
    #[turbo_tasks::function]
    async fn write_to_disk(self_vc: AppPageEndpointVc) -> Result<WrittenEndpointVc> {
        let node_root = self_vc.app_project().project().node_root();
        let node_root_ref = node_root.await?;

        let client_relative_path = self_vc.app_project().project().client_root().join("_next");
        let client_relative_path_ref = client_relative_path.await?;

        let server_path = self_vc.app_project().project().node_root().join("server");

        let mut output_assets = vec![];

        let client_shared_chunks = get_app_client_shared_chunks(
            self_vc.app_project().client_runtime_entries(),
            self_vc.app_project().project().client_chunking_context(),
        );

        let mut client_shared_chunks_paths = vec![];
        for chunk in client_shared_chunks.await?.iter().copied() {
            output_assets.push(chunk);

            let chunk_path = chunk.ident().path().await?;
            if chunk_path.extension() == Some("js") {
                if let Some(chunk_path) = client_relative_path_ref.get_path_to(&chunk_path) {
                    client_shared_chunks_paths.push(chunk_path.to_string());
                }
            }
        }

        let app_entry = self_vc.app_entry().await?;

        let rsc_entry = app_entry.rsc_entry;

        let rsc_entry_asset = rsc_entry.as_asset();
        let client_reference_graph =
            ClientReferenceGraphVc::new(AssetsVc::cell(vec![rsc_entry_asset]));
        let client_reference_types = client_reference_graph.types();
        let client_references = client_reference_graph.entry(rsc_entry_asset);

        let app_ssr_entries: Vec<_> = client_reference_types
            .await?
            .iter()
            .map(|client_reference_ty| async move {
                let ClientReferenceType::EcmascriptClientReference(entry) = client_reference_ty
                else {
                    return Ok(None);
                };

                Ok(Some(entry.await?.ssr_module))
            })
            .try_join()
            .await?
            .into_iter()
            .flatten()
            .collect();

        let app_node_entries: Vec<_> = app_ssr_entries.iter().copied().chain([rsc_entry]).collect();

        // TODO(alexkirsz) Handle dynamic entries and dynamic chunks.
        let _dynamic_entries = NextDynamicEntriesVc::from_entries(AssetsVc::cell(
            app_node_entries
                .iter()
                .copied()
                .map(|entry| entry.into())
                .collect(),
        ))
        .await?;

        let rsc_chunk = self_vc
            .app_project()
            .project()
            .rsc_chunking_context()
            .entry_chunk(
                server_path.join(&format!(
                    "app/{original_name}.js",
                    original_name = app_entry.original_name
                )),
                app_entry.rsc_entry,
                self_vc.app_project().rsc_runtime_entries(),
            );
        output_assets.push(rsc_chunk);

        let app_entry_client_references = client_reference_graph
            .entry(app_entry.rsc_entry.as_asset())
            .await?;

        let client_references_chunks = get_app_client_references_chunks(
            client_reference_types,
            self_vc.app_project().project().client_chunking_context(),
            self_vc.app_project().project().ssr_chunking_context(),
        );
        let client_references_chunks_ref = client_references_chunks.await?;

        let mut entry_client_chunks = vec![];
        // TODO(alexkirsz) In which manifest does this go?
        let mut entry_ssr_chunks = vec![];
        for client_reference in app_entry_client_references.iter() {
            let client_reference_chunks = client_references_chunks_ref
                .get(client_reference.ty())
                .expect("client reference should have corresponding chunks");
            entry_client_chunks
                .extend(client_reference_chunks.client_chunks.await?.iter().copied());
            entry_ssr_chunks.extend(client_reference_chunks.ssr_chunks.await?.iter().copied());
        }

        output_assets.extend(entry_client_chunks.iter().copied());
        output_assets.extend(entry_ssr_chunks.iter().copied());

        let entry_client_chunks_paths = entry_client_chunks
            .iter()
            .map(|chunk| chunk.ident().path())
            .try_join()
            .await?;
        let mut entry_client_chunks_paths: Vec<_> = entry_client_chunks_paths
            .iter()
            .map(|path| {
                client_relative_path_ref
                    .get_path_to(path)
                    .expect("asset path should be inside client root")
                    .to_string()
            })
            .collect();
        entry_client_chunks_paths.extend(client_shared_chunks_paths.iter().cloned());

        let mut app_build_manifest = AppBuildManifest::default();
        app_build_manifest
            .pages
            .insert(app_entry.original_name.clone(), entry_client_chunks_paths);
        let app_build_manifest_output = RawOutputVc::new(
            VirtualSourceVc::new(
                node_root.join(&format!("server/app-build-manifest.json",)),
                File::from(serde_json::to_string_pretty(&app_build_manifest)?).into(),
            )
            .into(),
        )
        .into();
        output_assets.push(app_build_manifest_output);

        let mut app_paths_manifest = AppPathsManifest::default();
        app_paths_manifest.node_server_app_paths.pages.insert(
            app_entry.original_name.clone(),
            server_path
                .await?
                .get_path_to(&*rsc_chunk.ident().path().await?)
                .expect("RSC chunk path should be within app paths manifest directory")
                .to_string(),
        );
        let app_paths_manifest_output = RawOutputVc::new(
            VirtualSourceVc::new(
                node_root.join(&format!("server/app-paths-manifest.json",)),
                File::from(serde_json::to_string_pretty(&app_paths_manifest)?).into(),
            )
            .into(),
        )
        .into();
        output_assets.push(app_paths_manifest_output);

        let mut build_manifest = BuildManifest::default();
        build_manifest.root_main_files = client_shared_chunks_paths;
        let build_manifest_output = RawOutputVc::new(
            VirtualSourceVc::new(
                node_root.join(&format!("build-manifest.json",)),
                File::from(serde_json::to_string_pretty(&build_manifest)?).into(),
            )
            .into(),
        )
        .into();
        output_assets.push(build_manifest_output);

        let entry_manifest = ClientReferenceManifest::build_output(
            node_root,
            client_relative_path,
            app_entry.original_name.clone(),
            client_references,
            client_references_chunks,
            self_vc.app_project().project().client_chunking_context(),
            self_vc
                .app_project()
                .project()
                .ssr_chunking_context()
                .into(),
        );
        output_assets.push(entry_manifest);

        // TODO(alexkirsz) Write chunks and manifests.

        emit_all_assets(
            OutputAssetsVc::cell(output_assets),
            self_vc.app_project().project().node_root(),
            client_relative_path,
            self_vc.app_project().project().node_root(),
        )
        .await?;

        Ok(WrittenEndpoint {
            server_entry_path: node_root_ref
                .get_path_to(&*rsc_chunk.ident().path().await?)
                .context("rsc chunk entry path must be inside the node root")?
                .to_string(),
            server_paths: vec![],
        }
        .into())
    }

    #[turbo_tasks::function]
    fn changed(&self) -> CompletionVc {
        todo!()
    }
}

#[turbo_tasks::value]
struct AppRouteEndpoint;

#[turbo_tasks::value_impl]
impl Endpoint for AppRouteEndpoint {
    #[turbo_tasks::function]
    fn write_to_disk(&self) -> WrittenEndpointVc {
        todo!()
    }

    #[turbo_tasks::function]
    fn changed(&self) -> CompletionVc {
        todo!()
    }
}
