use anyhow::Result;
use async_graphql::dynamic::{
    Field, Object, Scalar, Schema, Subscription, SubscriptionField, Union,
};
use sqlx::SqlitePool;
use torii_core::types::Model;

use super::object::connection::page_info::PageInfoObject;
use super::object::entity::EntityObject;
use super::object::event::EventObject;
use super::object::model_data::ModelDataObject;
use super::object::system::SystemObject;
use super::object::system_call::SystemCallObject;
use super::object::ObjectTrait;
use super::types::ScalarType;
use crate::object::model::ModelObject;
use crate::query::type_mapping_query;

// The graphql schema is built dynamically at runtime, this is because we won't know the schema of
// the models until runtime. There are however, predefined objects such as entities and
// events, their schema is known but we generate them dynamically as well because async-graphql
// does not allow mixing of static and dynamic schemas.
pub async fn build_schema(pool: &SqlitePool) -> Result<Schema> {
    let mut schema_builder = Schema::build("Query", None, Some("Subscription"));

    // predefined objects
    let mut objects: Vec<Box<dyn ObjectTrait>> = vec![
        Box::<EntityObject>::default(),
        Box::<ModelObject>::default(),
        Box::<SystemObject>::default(),
        Box::<EventObject>::default(),
        Box::<SystemCallObject>::default(),
        Box::<PageInfoObject>::default(),
    ];

    // build model data gql objects
    let (data_objects, data_union) = build_data_objects(pool).await?;
    objects.extend(data_objects);

    // register model data unions
    schema_builder = schema_builder.register(data_union);

    // register default scalars
    for scalar_type in ScalarType::all().iter() {
        schema_builder = schema_builder.register(Scalar::new(scalar_type));
    }

    // collect resolvers for single and plural queries
    let queries: Vec<Field> = objects
        .iter()
        .flat_map(|object| vec![object.resolve_one(), object.resolve_many()].into_iter().flatten())
        .collect();

    // add field resolvers to query root
    let mut query_root = Object::new("Query");
    for query in queries {
        query_root = query_root.field(query);
    }

    for object in &objects {
        // register enum objects
        if let Some(input_objects) = object.enum_objects() {
            for input in input_objects {
                schema_builder = schema_builder.register(input);
            }
        }

        // register input objects, whereInput and orderBy
        if let Some(input_objects) = object.input_objects() {
            for input in input_objects {
                schema_builder = schema_builder.register(input);
            }
        }

        // register connection types, relay
        if let Some(conn_objects) = object.connection() {
            for object in conn_objects {
                schema_builder = schema_builder.register(object);
            }
        }

        // register gql objects
        let object_collection = object.objects();
        for object in object_collection {
            schema_builder = schema_builder.register(object);
        }
    }

    // collect resolvers for single subscriptions
    let mut subscription_fields: Vec<SubscriptionField> = Vec::new();
    for object in &objects {
        if let Some(subscriptions) = object.subscriptions() {
            for sub in subscriptions {
                subscription_fields.push(sub);
            }
        }
    }

    // add field resolvers to subscription root
    let mut subscription_root = Subscription::new("Subscription");
    for field in subscription_fields {
        subscription_root = subscription_root.field(field);
    }

    schema_builder
        .register(query_root)
        .register(subscription_root)
        .data(pool.clone())
        .finish()
        .map_err(|e| e.into())
}

async fn build_data_objects(pool: &SqlitePool) -> Result<(Vec<Box<dyn ObjectTrait>>, Union)> {
    let mut conn = pool.acquire().await?;
    let mut objects: Vec<Box<dyn ObjectTrait>> = Vec::new();

    let models: Vec<Model> = sqlx::query_as("SELECT * FROM models").fetch_all(&mut conn).await?;

    // model union object
    let mut union = Union::new("ModelUnion");

    // model state objects
    for model in models {
        let type_mapping = type_mapping_query(&mut conn, &model.id).await?;

        if !type_mapping.is_empty() {
            let field_name = model.name.to_lowercase();
            let type_name = model.name;

            union = union.possible_type(&type_name);

            objects.push(Box::new(ModelDataObject::new(field_name, type_name, type_mapping)));
        }
    }

    Ok((objects, union))
}
