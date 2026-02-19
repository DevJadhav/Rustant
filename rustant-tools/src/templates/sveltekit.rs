//! SvelteKit + Tailwind CSS project template.

use super::{ProjectTemplate, TemplateFile};

pub fn template() -> ProjectTemplate {
    ProjectTemplate {
        name: "sveltekit".into(),
        description: "SvelteKit + Tailwind CSS".into(),
        framework: "SvelteKit".into(),
        files: vec![
            TemplateFile {
                path: "package.json".into(),
                content: r#"{
  "name": "{{project-name}}",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "vite dev",
    "build": "vite build",
    "preview": "vite preview",
    "check": "svelte-kit sync && svelte-check --tsconfig ./tsconfig.json",
    "lint": "eslint .",
    "test": "vitest"
  },
  "devDependencies": {
    "@sveltejs/adapter-auto": "^3.2.2",
    "@sveltejs/kit": "^2.5.18",
    "@sveltejs/vite-plugin-svelte": "^3.1.1",
    "autoprefixer": "^10.4.19",
    "eslint": "^9.5.0",
    "postcss": "^8.4.38",
    "svelte": "^4.2.18",
    "svelte-check": "^3.8.4",
    "tailwindcss": "^3.4.4",
    "typescript": "^5.5.2",
    "vite": "^5.3.1",
    "vitest": "^1.6.0"
  }
}"#
                .into(),
            },
            TemplateFile {
                path: "svelte.config.js".into(),
                content: r#"import adapter from '@sveltejs/adapter-auto'
import { vitePreprocess } from '@sveltejs/vite-plugin-svelte'

/** @type {import('@sveltejs/kit').Config} */
const config = {
  preprocess: vitePreprocess(),
  kit: {
    adapter: adapter(),
  },
}

export default config
"#
                .into(),
            },
            TemplateFile {
                path: "vite.config.ts".into(),
                content: r#"import { sveltekit } from '@sveltejs/kit/vite'
import { defineConfig } from 'vite'

export default defineConfig({
  plugins: [sveltekit()],
})
"#
                .into(),
            },
            TemplateFile {
                path: "tsconfig.json".into(),
                content: r#"{
  "extends": "./.svelte-kit/tsconfig.json",
  "compilerOptions": {
    "allowJs": true,
    "checkJs": true,
    "esModuleInterop": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "skipLibCheck": true,
    "sourceMap": true,
    "strict": true
  }
}"#
                .into(),
            },
            TemplateFile {
                path: "tailwind.config.js".into(),
                content: r#"/** @type {import('tailwindcss').Config} */
export default {
  content: ['./src/**/*.{html,js,svelte,ts}'],
  theme: {
    extend: {},
  },
  plugins: [],
}
"#
                .into(),
            },
            TemplateFile {
                path: "postcss.config.js".into(),
                content: r#"export default {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
}
"#
                .into(),
            },
            TemplateFile {
                path: "src/app.html".into(),
                content: r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1" />
    <title>{{ProjectName}}</title>
    %sveltekit.head%
  </head>
  <body data-sveltekit-preload-data="hover">
    <div style="display: contents">%sveltekit.body%</div>
  </body>
</html>
"#
                .into(),
            },
            TemplateFile {
                path: "src/app.css".into(),
                content: r#"@tailwind base;
@tailwind components;
@tailwind utilities;
"#
                .into(),
            },
            TemplateFile {
                path: "src/routes/+layout.svelte".into(),
                content: r#"<script>
  import '../app.css'
</script>

<slot />
"#
                .into(),
            },
            TemplateFile {
                path: "src/routes/+page.svelte".into(),
                content: r#"<main class="min-h-screen flex items-center justify-center bg-gray-50">
  <div class="text-center">
    <h1 class="text-4xl font-bold text-gray-900">{{ProjectName}}</h1>
    <p class="mt-4 text-gray-600">Edit src/routes/+page.svelte to get started</p>
  </div>
</main>
"#
                .into(),
            },
            TemplateFile {
                path: ".gitignore".into(),
                content: "node_modules\n.svelte-kit\nbuild\n.env\n".into(),
            },
        ],
        post_install: vec!["npm install".into()],
    }
}
