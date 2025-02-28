use std::io::Write;

use anyhow::{bail, Result};
use base64::{display::Base64Display, engine::general_purpose::STANDARD};
use indoc::writedoc;
use next_core::app_structure::MetadataItem;
use turbo_tasks::{ValueToString, Vc};
use turbopack_binding::{
    turbo::tasks_fs::{rope::RopeBuilder, File, FileContent, FileSystemPath},
    turbopack::{
        core::{asset::AssetContent, virtual_source::VirtualSource},
        ecmascript::utils::StringifyJs,
        turbopack::ModuleAssetContext,
    },
};

use super::{app_entries::AppEntry, app_route_entry::get_app_route_entry};

/// Computes the entry for a Next.js favicon file.
pub(super) async fn get_app_route_favicon_entry(
    rsc_context: Vc<ModuleAssetContext>,
    favicon: MetadataItem,
    project_root: Vc<FileSystemPath>,
) -> Result<Vc<AppEntry>> {
    let path = match favicon {
        // TODO(alexkirsz) Is there a difference here?
        MetadataItem::Static { path } => path,
        MetadataItem::Dynamic { path } => path,
    };

    let mut code = RopeBuilder::default();

    let content_type = mime_guess::from_ext(&path.extension().await?)
        .first_or_octet_stream()
        .to_string();
    let original_file_content = path.read().await?;
    let original_file_content_b64 = match &*original_file_content {
        FileContent::Content(content) => {
            let content = content.content().to_bytes()?;
            Base64Display::new(&content, &STANDARD).to_string()
        }
        FileContent::NotFound => {
            bail!("favicon file not found: {}", &path.to_string().await?);
        }
    };
    // Specific to favicon
    let cache_control = "public, max-age=0, must-revalidate";

    // TODO(alexkirsz) Generalize this to any file.
    writedoc! {
        code,
        r#"
            import {{ NextResponse }} from 'next/server'

            const contentType = {content_type}
            const cacheControl = {cache_control}
            const buffer = Buffer.from({original_file_content_b64}, 'base64')
            
            export function GET() {{
                return new NextResponse(buffer, {{
                    headers: {{
                        'Content-Type': contentType,
                        'Cache-Control': cacheControl,
                    }},
                }})
            }}
            
            export const dynamic = 'force-static'
            "#,
        content_type = StringifyJs(&content_type),
        cache_control = StringifyJs(&cache_control),
        original_file_content_b64 = StringifyJs(&original_file_content_b64),
    }?;

    let file = File::from(code.build());
    let source =
        // TODO(alexkirsz) Figure out how to name this virtual source.
        VirtualSource::new(project_root.join("todo.tsx".to_string()), AssetContent::file(file.into()));

    get_app_route_entry(
        rsc_context,
        Vc::upcast(source),
        // TODO(alexkirsz) Get this from the metadata?
        "/favicon.ico",
        project_root,
    )
    .await
}
