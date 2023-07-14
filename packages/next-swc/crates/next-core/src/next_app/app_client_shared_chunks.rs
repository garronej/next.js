use anyhow::Result;
use turbo_tasks::{TryJoinIterExt, Value};
use turbopack_binding::turbopack::{
    core::{
        chunk::{availability_info::AvailabilityInfo, ChunkingContext, EvaluatableAssetsVc},
        output::OutputAssetsVc,
    },
    ecmascript::chunk::{
        EcmascriptChunkPlaceableVc, EcmascriptChunkPlaceablesVc, EcmascriptChunkVc,
        EcmascriptChunkingContextVc,
    },
};

#[turbo_tasks::function]
pub async fn get_app_shared_client_chunk(
    app_client_runtime_entries: EvaluatableAssetsVc,
    client_chunking_context: EcmascriptChunkingContextVc,
) -> Result<EcmascriptChunkVc> {
    let client_runtime_entries: Vec<_> = app_client_runtime_entries
        .await?
        .iter()
        .map(|entry| async move { Ok(EcmascriptChunkPlaceableVc::resolve_from(*entry).await?) })
        .try_join()
        .await?
        .into_iter()
        .flatten()
        .collect();

    Ok(EcmascriptChunkVc::new_normalized(
        client_chunking_context,
        // TODO(alexkirsz) Should this accept Evaluatable instead?
        EcmascriptChunkPlaceablesVc::cell(client_runtime_entries),
        None,
        Value::new(AvailabilityInfo::Untracked),
    ))
}

#[turbo_tasks::function]
pub async fn get_app_client_shared_chunks(
    app_client_runtime_entries: EvaluatableAssetsVc,
    client_chunking_context: EcmascriptChunkingContextVc,
) -> Result<OutputAssetsVc> {
    if app_client_runtime_entries.await?.is_empty() {
        return Ok(OutputAssetsVc::empty());
    }

    let app_client_shared_chunk =
        get_app_shared_client_chunk(app_client_runtime_entries, client_chunking_context);

    let app_client_shared_chunks = client_chunking_context
        .evaluated_chunk_group(app_client_shared_chunk.into(), app_client_runtime_entries);

    Ok(app_client_shared_chunks)
}
