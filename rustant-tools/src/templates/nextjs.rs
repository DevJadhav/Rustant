//! Next.js 14 App Router + Tailwind CSS project template.

use super::{ProjectTemplate, TemplateFile};

pub fn template() -> ProjectTemplate {
    ProjectTemplate {
        name: "nextjs".into(),
        description: "Next.js 14 App Router + Tailwind CSS".into(),
        framework: "Next.js".into(),
        files: vec![
            TemplateFile {
                path: "package.json".into(),
                content: r#"{
  "name": "{{project-name}}",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "next dev",
    "build": "next build",
    "start": "next start",
    "lint": "next lint",
    "test": "vitest"
  },
  "dependencies": {
    "next": "14.2.4",
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  },
  "devDependencies": {
    "@types/node": "^20.14.8",
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "autoprefixer": "^10.4.19",
    "eslint": "^8.57.0",
    "eslint-config-next": "14.2.4",
    "postcss": "^8.4.38",
    "tailwindcss": "^3.4.4",
    "typescript": "^5.5.2",
    "vitest": "^1.6.0"
  }
}"#
                .into(),
            },
            TemplateFile {
                path: "tsconfig.json".into(),
                content: r#"{
  "compilerOptions": {
    "target": "ES2017",
    "lib": ["dom", "dom.iterable", "esnext"],
    "allowJs": true,
    "skipLibCheck": true,
    "strict": true,
    "noEmit": true,
    "esModuleInterop": true,
    "module": "esnext",
    "moduleResolution": "bundler",
    "resolveJsonModule": true,
    "isolatedModules": true,
    "jsx": "preserve",
    "incremental": true,
    "plugins": [{ "name": "next" }],
    "paths": { "@/*": ["./src/*"] }
  },
  "include": ["next-env.d.ts", "**/*.ts", "**/*.tsx", ".next/types/**/*.ts"],
  "exclude": ["node_modules"]
}"#
                .into(),
            },
            TemplateFile {
                path: "next.config.mjs".into(),
                content: r#"/** @type {import('next').NextConfig} */
const nextConfig = {}

export default nextConfig
"#
                .into(),
            },
            TemplateFile {
                path: "tailwind.config.ts".into(),
                content: r#"import type { Config } from 'tailwindcss'

const config: Config = {
  content: [
    './src/pages/**/*.{js,ts,jsx,tsx,mdx}',
    './src/components/**/*.{js,ts,jsx,tsx,mdx}',
    './src/app/**/*.{js,ts,jsx,tsx,mdx}',
  ],
  theme: {
    extend: {},
  },
  plugins: [],
}
export default config
"#
                .into(),
            },
            TemplateFile {
                path: "postcss.config.mjs".into(),
                content: r#"/** @type {import('postcss-load-config').Config} */
const config = {
  plugins: {
    tailwindcss: {},
    autoprefixer: {},
  },
}
export default config
"#
                .into(),
            },
            TemplateFile {
                path: "src/app/layout.tsx".into(),
                content: r#"import type { Metadata } from 'next'
import './globals.css'

export const metadata: Metadata = {
  title: '{{ProjectName}}',
  description: 'Created with Rustant',
}

export default function RootLayout({
  children,
}: {
  children: React.ReactNode
}) {
  return (
    <html lang="en">
      <body>{children}</body>
    </html>
  )
}
"#
                .into(),
            },
            TemplateFile {
                path: "src/app/page.tsx".into(),
                content: r#"export default function Home() {
  return (
    <main className="min-h-screen flex items-center justify-center bg-gray-50">
      <div className="text-center">
        <h1 className="text-4xl font-bold text-gray-900">{{ProjectName}}</h1>
        <p className="mt-4 text-gray-600">Edit src/app/page.tsx to get started</p>
      </div>
    </main>
  )
}
"#
                .into(),
            },
            TemplateFile {
                path: "src/app/globals.css".into(),
                content: r#"@tailwind base;
@tailwind components;
@tailwind utilities;
"#
                .into(),
            },
            TemplateFile {
                path: ".gitignore".into(),
                content: "node_modules\n.next\nout\n.env\n.env.local\n".into(),
            },
        ],
        post_install: vec!["npm install".into()],
    }
}
