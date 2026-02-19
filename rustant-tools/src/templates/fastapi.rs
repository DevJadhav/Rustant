//! FastAPI + SQLAlchemy + Alembic project template.

use super::{ProjectTemplate, TemplateFile};

pub fn template() -> ProjectTemplate {
    ProjectTemplate {
        name: "fastapi".into(),
        description: "FastAPI + SQLAlchemy + Alembic".into(),
        framework: "FastAPI".into(),
        files: vec![
            TemplateFile {
                path: "pyproject.toml".into(),
                content: r#"[project]
name = "{{project-name}}"
version = "0.1.0"
description = ""
requires-python = ">=3.11"
dependencies = [
    "fastapi>=0.111.0",
    "uvicorn[standard]>=0.30.1",
    "sqlalchemy>=2.0.31",
    "alembic>=1.13.1",
    "pydantic-settings>=2.3.4",
    "python-dotenv>=1.0.1",
]

[project.optional-dependencies]
dev = [
    "pytest>=8.2.2",
    "httpx>=0.27.0",
    "ruff>=0.4.8",
    "mypy>=1.10.0",
]

[tool.ruff]
target-version = "py311"
line-length = 88

[tool.ruff.lint]
select = ["E", "F", "I", "N", "W", "UP"]

[tool.mypy]
python_version = "3.11"
strict = true
"#
                .into(),
            },
            TemplateFile {
                path: "app/__init__.py".into(),
                content: String::new(),
            },
            TemplateFile {
                path: "app/main.py".into(),
                content: r#"from fastapi import FastAPI

app = FastAPI(title="{{ProjectName}}")


@app.get("/")
async def root():
    return {"message": "Hello from {{ProjectName}}"}


@app.get("/health")
async def health():
    return {"status": "ok"}
"#
                .into(),
            },
            TemplateFile {
                path: "app/config.py".into(),
                content: r#"from pydantic_settings import BaseSettings


class Settings(BaseSettings):
    app_name: str = "{{ProjectName}}"
    database_url: str = "sqlite:///./app.db"
    debug: bool = False

    class Config:
        env_file = ".env"


settings = Settings()
"#
                .into(),
            },
            TemplateFile {
                path: "app/database.py".into(),
                content: r#"from sqlalchemy import create_engine
from sqlalchemy.orm import DeclarativeBase, sessionmaker

from app.config import settings

engine = create_engine(settings.database_url, echo=settings.debug)
SessionLocal = sessionmaker(autocommit=False, autoflush=False, bind=engine)


class Base(DeclarativeBase):
    pass


def get_db():
    db = SessionLocal()
    try:
        yield db
    finally:
        db.close()
"#
                .into(),
            },
            TemplateFile {
                path: "app/models.py".into(),
                content: r#"from sqlalchemy import Column, Integer, String

from app.database import Base


class Item(Base):
    __tablename__ = "items"

    id = Column(Integer, primary_key=True, index=True)
    name = Column(String, nullable=False)
    description = Column(String, default="")
"#
                .into(),
            },
            TemplateFile {
                path: "app/routers/__init__.py".into(),
                content: String::new(),
            },
            TemplateFile {
                path: "tests/__init__.py".into(),
                content: String::new(),
            },
            TemplateFile {
                path: "tests/test_main.py".into(),
                content: r#"from fastapi.testclient import TestClient

from app.main import app

client = TestClient(app)


def test_root():
    response = client.get("/")
    assert response.status_code == 200


def test_health():
    response = client.get("/health")
    assert response.status_code == 200
    assert response.json()["status"] == "ok"
"#
                .into(),
            },
            TemplateFile {
                path: "alembic.ini".into(),
                content: r#"[alembic]
script_location = migrations
sqlalchemy.url = sqlite:///./app.db
"#
                .into(),
            },
            TemplateFile {
                path: ".env.example".into(),
                content: "DATABASE_URL=sqlite:///./app.db\nDEBUG=true\n".into(),
            },
            TemplateFile {
                path: ".gitignore".into(),
                content: "__pycache__\n*.pyc\n.env\n*.db\n.venv\ndist\n*.egg-info\n.mypy_cache\n.ruff_cache\n".into(),
            },
        ],
        post_install: vec![
            "python -m venv .venv".into(),
            ".venv/bin/pip install -e '.[dev]'".into(),
        ],
    }
}
