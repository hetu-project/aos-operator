use sea_orm_migration::prelude::*;

#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(JobResultParam::Table)
                    .if_not_exists()
                    .col(
                        ColumnDef::new(JobResultParam::Id)
                            .big_integer()
                            .not_null()
                            .auto_increment()
                            .primary_key(),
                    )
                    .col(ColumnDef::new(JobResultParam::Username).string().not_null())
                    .col(
                        ColumnDef::new(JobResultParam::JobId)
                            .string()
                            .unique_key()
                            .not_null(),
                    )
                    .col(ColumnDef::new(JobResultParam::Result).string().not_null())
                    .col(ColumnDef::new(JobResultParam::Tag).string().not_null())
                    .col(ColumnDef::new(JobResultParam::Clock).string().not_null())
                    .col(ColumnDef::new(JobResultParam::Operator).string().not_null())
                    .col(
                        ColumnDef::new(JobResultParam::Signature)
                            .string()
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(JobResultParam::Table).to_owned())
            .await
    }
}

#[derive(DeriveIden)]
enum JobResultParam {
    Id,
    Table,
    Username,
    JobId,
    Result,
    Tag,
    Clock,
    Operator,
    Signature,
}
