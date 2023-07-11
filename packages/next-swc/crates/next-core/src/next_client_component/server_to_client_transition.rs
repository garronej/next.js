use anyhow::Result;
use indexmap::indexmap;
use turbo_tasks::Value;
use turbopack_binding::turbopack::{
    core::{
        context::AssetContext,
        module::ModuleVc,
        reference_type::{EntryReferenceSubType, InnerAssetsVc, ReferenceType},
        source::SourceVc,
    },
    turbopack::{
        transition::{Transition, TransitionVc},
        ModuleAssetContextVc,
    },
};

use crate::embed_js::next_asset;

#[turbo_tasks::value(shared)]
pub struct NextServerToClientTransition {
    pub ssr: bool,
}

#[turbo_tasks::value_impl]
impl Transition for NextServerToClientTransition {
    #[turbo_tasks::function]
    async fn process(
        self_vc: NextServerToClientTransitionVc,
        source: SourceVc,
        context: ModuleAssetContextVc,
        _reference_type: Value<ReferenceType>,
    ) -> Result<ModuleVc> {
        let internal_source = next_asset(if self_vc.await?.ssr {
            "entry/app/server-to-client-ssr.tsx"
        } else {
            "entry/app/server-to-client.tsx"
        });
        let context = self_vc.process_context(context);
        let client_chunks = context.with_transition("next-client-chunks").process(
            source,
            Value::new(ReferenceType::Entry(
                EntryReferenceSubType::AppClientComponent,
            )),
        );
        let client_module = context.with_transition("next-ssr-client-module").process(
            source,
            Value::new(ReferenceType::Entry(
                EntryReferenceSubType::AppClientComponent,
            )),
        );
        Ok(context.process(
            internal_source,
            Value::new(ReferenceType::Internal(InnerAssetsVc::cell(indexmap! {
                "CLIENT_MODULE".to_string() => client_module.into(),
                "CLIENT_CHUNKS".to_string() => client_chunks.into(),
            }))),
        ))
    }
}
