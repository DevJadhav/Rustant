//! Classical ML algorithm definitions (executed via Python subprocess).

use serde::{Deserialize, Serialize};

/// Classical ML algorithms (sklearn-compatible).
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClassicalAlgorithm {
    LinearRegression,
    LogisticRegression,
    Ridge {
        alpha: f64,
    },
    Lasso {
        alpha: f64,
    },
    DecisionTree {
        max_depth: Option<usize>,
    },
    RandomForest {
        n_estimators: usize,
        max_depth: Option<usize>,
    },
    GradientBoosting {
        n_estimators: usize,
        learning_rate: f64,
    },
    XGBoost {
        n_estimators: usize,
        learning_rate: f64,
        max_depth: usize,
    },
    KMeans {
        n_clusters: usize,
    },
    Dbscan {
        eps: f64,
        min_samples: usize,
    },
    Pca {
        n_components: usize,
    },
    Tsne {
        n_components: usize,
        perplexity: f64,
    },
    Svm {
        kernel: String,
        c: f64,
    },
    Knn {
        n_neighbors: usize,
    },
    NaiveBayes,
    Hierarchical {
        n_clusters: usize,
        linkage: String,
    },
    Umap {
        n_components: usize,
        n_neighbors: usize,
    },
}

impl ClassicalAlgorithm {
    pub fn sklearn_class(&self) -> &str {
        match self {
            Self::LinearRegression => "sklearn.linear_model.LinearRegression",
            Self::LogisticRegression => "sklearn.linear_model.LogisticRegression",
            Self::Ridge { .. } => "sklearn.linear_model.Ridge",
            Self::Lasso { .. } => "sklearn.linear_model.Lasso",
            Self::DecisionTree { .. } => "sklearn.tree.DecisionTreeClassifier",
            Self::RandomForest { .. } => "sklearn.ensemble.RandomForestClassifier",
            Self::GradientBoosting { .. } => "sklearn.ensemble.GradientBoostingClassifier",
            Self::XGBoost { .. } => "xgboost.XGBClassifier",
            Self::KMeans { .. } => "sklearn.cluster.KMeans",
            Self::Dbscan { .. } => "sklearn.cluster.DBSCAN",
            Self::Pca { .. } => "sklearn.decomposition.PCA",
            Self::Tsne { .. } => "sklearn.manifold.TSNE",
            Self::Svm { .. } => "sklearn.svm.SVC",
            Self::Knn { .. } => "sklearn.neighbors.KNeighborsClassifier",
            Self::NaiveBayes => "sklearn.naive_bayes.GaussianNB",
            Self::Hierarchical { .. } => "sklearn.cluster.AgglomerativeClustering",
            Self::Umap { .. } => "umap.UMAP",
        }
    }
}
