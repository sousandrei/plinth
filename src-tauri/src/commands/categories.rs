use serde::{Deserialize, Serialize};
use tauri::State;
use uuid::Uuid;

use crate::{Session, db::DbPool, error::AppError};

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::FromRow)]
pub struct Category {
    pub id: String,
    pub name: String,
    pub color: String,
}

#[tauri::command]
pub async fn list_all_categories(
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<Vec<Category>, AppError> {
    let active = session.require()?;

    let categories = sqlx::query_file_as!(
        Category,
        "queries/categories/list_categories.sql",
        active.space_id
    )
    .fetch_all(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("list_all_categories: {e}")))?;

    Ok(categories)
}

#[tauri::command]
pub async fn create_category(
    name: String,
    color: String,
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<Category, AppError> {
    let active = session.require()?;

    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("name cannot be empty".into()));
    }
    if color.trim().is_empty() {
        return Err(AppError::InvalidInput("color cannot be empty".into()));
    }
    let name = name.trim().to_string();
    let color = color.trim().to_string();
    let id = Uuid::new_v4().to_string();

    sqlx::query_file!(
        "queries/categories/create_category.sql",
        id,
        name,
        color,
        active.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("create_category: {e}")))?;

    Ok(Category { id, name, color })
}

#[tauri::command]
pub async fn delete_category(
    id: String,
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<(), AppError> {
    let active = session.require()?;

    let category = sqlx::query_file_as!(
        Category,
        "queries/categories/get_category_by_id.sql",
        id,
        active.space_id
    )
    .fetch_one(db.inner())
    .await
    .map_err(|e| AppError::NotFound(format!("category {id} not found: {e}")))?;

    sqlx::query_file!(
        "queries/categories/clear_transaction_category.sql",
        category.name,
        active.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("clear transactions category: {e}")))?;

    sqlx::query_file!(
        "queries/categories/delete_category.sql",
        id,
        active.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("delete_category: {e}")))?;

    Ok(())
}

#[tauri::command]
pub async fn update_category(
    id: String,
    name: String,
    color: String,
    db: State<'_, DbPool>,
    session: State<'_, Session>,
) -> Result<Category, AppError> {
    let active = session.require()?;

    if name.trim().is_empty() {
        return Err(AppError::InvalidInput("name cannot be empty".into()));
    }
    if color.trim().is_empty() {
        return Err(AppError::InvalidInput("color cannot be empty".into()));
    }
    let new_name = name.trim().to_string();
    let new_color = color.trim().to_string();

    let old_category = sqlx::query_file_as!(
        Category,
        "queries/categories/get_category_by_id.sql",
        id,
        active.space_id
    )
    .fetch_one(db.inner())
    .await
    .map_err(|e| AppError::NotFound(format!("category {id} not found: {e}")))?;

    if old_category.name != new_name {
        sqlx::query_file!(
            "queries/categories/update_transaction_categories.sql",
            old_category.name,
            new_name,
            active.space_id
        )
        .execute(db.inner())
        .await
        .map_err(|e| AppError::Db(format!("update transactions category: {e}")))?;
    }

    sqlx::query_file!(
        "queries/categories/update_category.sql",
        new_name,
        new_color,
        id,
        active.space_id
    )
    .execute(db.inner())
    .await
    .map_err(|e| AppError::Db(format!("update_category: {e}")))?;

    Ok(Category {
        id,
        name: new_name,
        color: new_color,
    })
}
