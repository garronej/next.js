use anyhow::Result;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use turbo_tasks::{debug::ValueDebugFormat, trace::TraceRawVcs, TryJoinIterExt};
use turbopack_binding::turbopack::{
    build::BuildChunkingContextVc,
    core::{
        chunk::{ChunkableModule, ChunkingContext},
        output::OutputAssetsVc,
    },
    ecmascript::chunk::EcmascriptChunkingContextVc,
};

use crate::next_client_reference::{ClientReferenceType, ClientReferenceTypesVc};

/// Contains the chunks corresponding to a client reference.
#[derive(
    Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, TraceRawVcs, ValueDebugFormat,
)]
pub struct ClientReferenceChunks {
    /// Chunks to be loaded on the client.
    pub client_chunks: OutputAssetsVc,
    /// Chunks to be loaded on the server for SSR.
    pub ssr_chunks: OutputAssetsVc,
}

#[turbo_tasks::value(transparent)]
pub struct ClientReferencesChunks(IndexMap<ClientReferenceType, ClientReferenceChunks>);

/// Computes all client references chunks.
///
/// This returns a map from client reference type to the chunks that reference
/// type needs to load.
#[turbo_tasks::function]
pub async fn get_app_client_references_chunks(
    app_client_reference_types: ClientReferenceTypesVc,
    client_chunking_context: EcmascriptChunkingContextVc,
    ssr_chunking_context: BuildChunkingContextVc,
) -> Result<ClientReferencesChunksVc> {
    let app_client_references_chunks: IndexMap<_, _> = app_client_reference_types
        .await?
        .iter()
        .map(|client_reference_ty| async move {
            Ok((
                *client_reference_ty,
                match client_reference_ty {
                    ClientReferenceType::EcmascriptClientReference(ecmascript_client_reference) => {
                        let ecmascript_client_reference_ref = ecmascript_client_reference.await?;
                        let client_entry_chunk = ecmascript_client_reference_ref
                            .client_module
                            .as_root_chunk(client_chunking_context.into());
                        let ssr_entry_chunk = ecmascript_client_reference_ref
                            .ssr_module
                            .as_root_chunk(ssr_chunking_context.into());
                        ClientReferenceChunks {
                            client_chunks: client_chunking_context.chunk_group(client_entry_chunk),
                            ssr_chunks: ssr_chunking_context.chunk_group(ssr_entry_chunk),
                        }
                    }
                    ClientReferenceType::CssClientReference(css_client_reference) => {
                        let css_client_reference_ref = css_client_reference.await?;
                        let client_entry_chunk = css_client_reference_ref
                            .client_module
                            .as_root_chunk(client_chunking_context.into());
                        ClientReferenceChunks {
                            client_chunks: client_chunking_context.chunk_group(client_entry_chunk),
                            ssr_chunks: OutputAssetsVc::empty(),
                        }
                    }
                },
            ))
        })
        .try_join()
        .await?
        .into_iter()
        .collect();

    Ok(ClientReferencesChunksVc::cell(app_client_references_chunks))
}
