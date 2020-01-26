extern crate libzfs;

fn main() {
    let dsname = std::env::args().nth(1).expect("specify a ZFS dataset name");
    let poolname = dsname.split('/').next().unwrap().to_owned();

    let client = libzfs::LibZfs::new().expect("lib fail");

    println!("Opening ZPool {:?}", poolname);
    let pool = client.pool_by_name(&poolname.into())
        .expect("pool fail");
    println!("{:?}", pool);
    println!("name: {:?}", pool.get_name());
    println!("state: {:?}", pool.get_state());

    println!();
    println!("Opening dataset {:?}", dsname);
    let ds = client.dataset_by_name(
            &dsname.into(),
            libzfs::DatasetType::Filesystem.into())
        .expect("dataset fail");
    println!("{:?}", ds);
    println!("name: {:?}", ds.get_name());
    println!("type: {:?}", ds.get_type());

    println!("sub filesystems:");
    for fs in ds.get_child_filesystems() {
        println!("\t{:?}", fs.get_name());
    }

    println!("snapshots:");
    ds.foreach_snapshot_ordered(Box::new(|snap| {
        println!("\t{:?}", snap.get_name());
    }));
}
