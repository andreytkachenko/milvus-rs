use milvus::{Client, Endpoint};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let endpoint = Endpoint::from_static("http://127.0.0.1:19530");
    let mut client = Client::connect("".into(), endpoint).await?;

    client.load_partitions("listings", ["RU"]).await?;

    let data = client
        .query("listings", "id >= 0 || id < 0", ["id"], ["RU"])
        .await?;

    println!("{:#?}", data);

    Ok(())
}
