use std::collections::BTreeMap;

use sea_orm::{
    ColumnTrait, ColumnType, DbBackend, DbErr, EntityTrait, IdenStatic, Iterable, QueryTrait,
    Select, SelectFive, SelectFour, SelectSix, SelectThree, SelectTwo, SelectTwoMany,
    SelectTwoRequired, Topology,
    sea_query::{Expr, SelectExpr, SelectStatement},
};

use crate::{BatchQuery, D1Type, Projection, error};

/// Derive named D1 projections from ordinary SeaORM entity/relation selects.
///
/// Handles entity columns and A_/B_/... relation aliases, including optional
/// joined entities. Filters/order/LIMIT/OFFSET and column subsets are supported.
/// Computed expressions, custom result aliases and custom select_as conversions
/// require an explicit Projection instead.
pub trait SelectProjection: QueryTrait<QueryStatement = SelectStatement> {
    fn projection(&self) -> Result<Projection, DbErr>;

    /// Build SQL and its projection together before any batch is dispatched.
    fn batch_query(&self, backend: DbBackend) -> Result<BatchQuery, DbErr> {
        let projection = self.projection()?;
        Ok(BatchQuery::new(self.build(backend), projection))
    }
}

struct SelectedColumn {
    source: Expr,
    kind: ColumnType,
    nullable: bool,
}

fn entity_columns<E: EntityTrait>(
    prefix: &str,
    nullable: bool,
) -> BTreeMap<String, SelectedColumn> {
    E::Column::iter()
        .map(|column| {
            let definition = column.def();
            (
                format!("{prefix}{}", column.as_str()),
                SelectedColumn {
                    source: column.select_as(column.into_expr()),
                    kind: definition.get_column_type().clone(),
                    nullable: nullable || definition.is_null(),
                },
            )
        })
        .collect()
}

fn column_name(expr: &Expr) -> Option<String> {
    match expr {
        Expr::Column(column) => column.column().map(ToString::to_string),
        Expr::AsEnum(_, inner) => column_name(inner),
        _ => None,
    }
}

fn selection_projection(
    query: &SelectStatement,
    columns: BTreeMap<String, SelectedColumn>,
) -> Result<Projection, DbErr> {
    // SeaQuery exposes a mutable visitor; visit a clone without modifying SQL.
    let mut selects: Vec<SelectExpr> = Vec::new();
    query
        .clone()
        .exprs_mut_for_each(|select| selects.push(select.clone()));
    let mut projection = Projection::new();
    for select in selects {
        let alias = select
            .alias
            .as_ref()
            .map(ToString::to_string)
            .or_else(|| column_name(&select.expr))
            .ok_or_else(|| error("computed SELECT expression requires an explicit Projection"))?;
        let column = columns
            .get(&alias)
            .filter(|column| select.window.is_none() && select.expr == column.source)
            .ok_or_else(|| {
                error(format!(
                    "custom SELECT alias `{alias}` requires an explicit Projection"
                ))
            })?;
        projection = projection.column(
            alias,
            D1Type::from_column_type(&column.kind)?,
            column.nullable,
        )?;
    }
    Ok(projection)
}

impl<E: EntityTrait> SelectProjection for Select<E> {
    fn projection(&self) -> Result<Projection, DbErr> {
        selection_projection(self.as_query(), entity_columns::<E>("", false))
    }
}

macro_rules! two_entities {
    ($select:ident, $optional:expr) => {
        impl<E: EntityTrait, F: EntityTrait> SelectProjection for $select<E, F> {
            fn projection(&self) -> Result<Projection, DbErr> {
                let mut columns = entity_columns::<E>("A_", false);
                columns.extend(entity_columns::<F>("B_", $optional));
                selection_projection(self.as_query(), columns)
            }
        }
    };
}

two_entities!(SelectTwo, true);
two_entities!(SelectTwoMany, true);
two_entities!(SelectTwoRequired, false);

macro_rules! more_entities {
    ($select:ident, $($entity:ident => $prefix:literal),+) => {
        impl<E: EntityTrait, $($entity: EntityTrait,)+ T: Topology> SelectProjection
            for $select<E, $($entity,)+ T>
        {
            fn projection(&self) -> Result<Projection, DbErr> {
                let mut columns = entity_columns::<E>("A_", false);
                $(columns.extend(entity_columns::<$entity>($prefix, true));)+
                selection_projection(self.as_query(), columns)
            }
        }
    };
}

more_entities!(SelectThree, F => "B_", G => "C_");
more_entities!(SelectFour, F => "B_", G => "C_", H => "D_");
more_entities!(SelectFive, F => "B_", G => "C_", H => "D_", I => "E_");
more_entities!(SelectSix, F => "B_", G => "C_", H => "D_", I => "E_", J => "F_");
