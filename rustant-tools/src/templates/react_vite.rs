//! React + Vite + Tailwind CSS project template.

use super::{ProjectTemplate, TemplateFile};

pub fn template() -> ProjectTemplate {
    ProjectTemplate {
        name: "react-vite".into(),
        description: "React + Vite + Tailwind CSS".into(),
        framework: "React".into(),
        files: vec![
            TemplateFile {
                path: "package.json".into(),
                content: r#"{
  "name": "{{project-name}}",
  "private": true,
  "version": "0.1.0",
  "type": "module",
  "scripts": {
    "dev": "vite",
    "build": "tsc && vite build",
    "lint": "eslint . --ext ts,tsx --report-unused-disable-directives --max-warnings 0",
    "preview": "vite preview",
    "test": "vitest"
  },
  "dependencies": {
    "react": "^18.3.1",
    "react-dom": "^18.3.1"
  },
  "devDependencies": {
    "@types/react": "^18.3.3",
    "@types/react-dom": "^18.3.0",
    "@vitejs/plugin-react": "^4.3.1",
    "autoprefixer": "^10.4.19",
    "eslint": "^9.5.0",
    "postcss": "^8.4.38",
    "tailwindcss": "^3.4.4",
    "typescript": "^5.5.2",
    "vite": "^5.3.1",
    "vitest": "^1.6.0"
  }
}"#
                .into(),
            },
            TemplateFile {
                path: "tsconfig.json".into(),
                content: r#"{
  "compilerOptions": {
    "target": "ES2020",
    "useDefineForClassFields": true,
    "lib": ["ES2020", "DOM", "DOM.Iterable"],
    "module": "ESNext",
    "skipLibCheck": true,
    "moduleResolution": "bundler",
    "allowImportingTsExtensions": true,
    "resolveJsonModule": true,
    "isolatedModules": true,
    "noEmit": true,
    "jsx": "react-jsx",
    "strict": true,
    "noUnusedLocals": true,
    "noUnusedParameters": true,
    "noFallthroughCasesInSwitch": true
  },
  "include": ["src"],
  "references": [{ "path": "./tsconfig.node.json" }]
}"#
                .into(),
            },
            TemplateFile {
                path: "tsconfig.node.json".into(),
                content: r#"{
  "compilerOptions": {
    "composite": true,
    "skipLibCheck": true,
    "module": "ESNext",
    "moduleResolution": "bundler",
    "allowSyntheticDefaultImports": true
  },
  "include": ["vite.config.ts"]
}"#
                .into(),
            },
            TemplateFile {
                path: "vite.config.ts".into(),
                content: r#"import { defineConfig } from 'vite'
import react from '@vitejs/plugin-react'

export default defineConfig({
  plugins: [react()],
})
"#
                .into(),
            },
            TemplateFile {
                path: "tailwind.config.js".into(),
                content: r#"/** @type {import('tailwindcss').Config} */
export default {
  content: ['./index.html', './src/**/*.{js,ts,jsx,tsx}'],
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
                path: "index.html".into(),
                content: r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>{{ProjectName}}</title>
  </head>
  <body>
    <div id="root"></div>
    <script type="module" src="/src/main.tsx"></script>
  </body>
</html>
"#
                .into(),
            },
            TemplateFile {
                path: "src/main.tsx".into(),
                content: r#"import React from 'react'
import ReactDOM from 'react-dom/client'
import App from './App'
import './index.css'

ReactDOM.createRoot(document.getElementById('root')!).render(
  <React.StrictMode>
    <App />
  </React.StrictMode>,
)
"#
                .into(),
            },
            TemplateFile {
                path: "src/App.tsx".into(),
                content: r#"function App() {
  return (
    <div className="min-h-screen bg-gray-50 flex items-center justify-center">
      <div className="text-center">
        <h1 className="text-4xl font-bold text-gray-900">{{ProjectName}}</h1>
        <p className="mt-4 text-gray-600">Edit src/App.tsx to get started</p>
      </div>
    </div>
  )
}

export default App
"#
                .into(),
            },
            TemplateFile {
                path: "src/index.css".into(),
                content: r#"@tailwind base;
@tailwind components;
@tailwind utilities;
"#
                .into(),
            },
            TemplateFile {
                path: "src/vite-env.d.ts".into(),
                content: r#"/// <reference types="vite/client" />
"#
                .into(),
            },
            TemplateFile {
                path: ".gitignore".into(),
                content: "node_modules\ndist\n*.local\n.env\n".into(),
            },
        ],
        post_install: vec!["npm install".into()],
    }
}
