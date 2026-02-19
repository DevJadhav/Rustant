//! Express.js + TypeScript + Prisma project template.

use super::{ProjectTemplate, TemplateFile};

pub fn template() -> ProjectTemplate {
    ProjectTemplate {
        name: "express".into(),
        description: "Express.js + TypeScript + Prisma".into(),
        framework: "Express".into(),
        files: vec![
            TemplateFile {
                path: "package.json".into(),
                content: r#"{
  "name": "{{project-name}}",
  "version": "0.1.0",
  "private": true,
  "scripts": {
    "dev": "tsx watch src/index.ts",
    "build": "tsc",
    "start": "node dist/index.js",
    "lint": "eslint src --ext .ts",
    "test": "vitest",
    "db:migrate": "prisma migrate dev",
    "db:generate": "prisma generate"
  },
  "dependencies": {
    "@prisma/client": "^5.15.0",
    "express": "^4.19.2",
    "zod": "^3.23.8"
  },
  "devDependencies": {
    "@types/express": "^4.17.21",
    "@types/node": "^20.14.8",
    "eslint": "^9.5.0",
    "prisma": "^5.15.0",
    "tsx": "^4.15.7",
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
    "target": "ES2022",
    "module": "NodeNext",
    "moduleResolution": "nodenext",
    "outDir": "dist",
    "rootDir": "src",
    "strict": true,
    "esModuleInterop": true,
    "skipLibCheck": true,
    "forceConsistentCasingInFileNames": true,
    "resolveJsonModule": true,
    "declaration": true,
    "declarationMap": true,
    "sourceMap": true
  },
  "include": ["src"],
  "exclude": ["node_modules", "dist"]
}"#
                .into(),
            },
            TemplateFile {
                path: "src/index.ts".into(),
                content: r#"import express from 'express'
import { router } from './routes.js'

const app = express()
const port = process.env.PORT || 3000

app.use(express.json())
app.use('/', router)

app.listen(port, () => {
  console.log(`Server running on http://localhost:${port}`)
})

export { app }
"#
                .into(),
            },
            TemplateFile {
                path: "src/routes.ts".into(),
                content: r#"import { Router } from 'express'

export const router = Router()

router.get('/', (_req, res) => {
  res.json({ message: 'Hello from {{ProjectName}}' })
})

router.get('/health', (_req, res) => {
  res.json({ status: 'ok' })
})
"#
                .into(),
            },
            TemplateFile {
                path: "prisma/schema.prisma".into(),
                content: r#"generator client {
  provider = "prisma-client-js"
}

datasource db {
  provider = "sqlite"
  url      = env("DATABASE_URL")
}

model Item {
  id          Int      @id @default(autoincrement())
  name        String
  description String   @default("")
  createdAt   DateTime @default(now())
}
"#
                .into(),
            },
            TemplateFile {
                path: ".env.example".into(),
                content: "DATABASE_URL=\"file:./dev.db\"\nPORT=3000\n".into(),
            },
            TemplateFile {
                path: ".gitignore".into(),
                content: "node_modules\ndist\n.env\n*.db\nprisma/migrations\n".into(),
            },
        ],
        post_install: vec!["npm install".into(), "npx prisma generate".into()],
    }
}
