extern crate libzfs;

fn main() {
    let dsname = std::env::args().nth(1).expect("specify a ZFS dataset name");
    let poolname = dsname.split('/').next().unwrap().to_owned();

    let client = libzfs::LibZfs::new().expect("lib fail");

    println!("Opening ZPool {:?}", poolname);
    let pool = client.pool_by_name(&poolname.into())
        .expect("pool fail");
    println!("{:?}", pool);
    println!("{:?}", pool.get_name());
    println!("{:?}", pool.get_state());

    println!();
    println!("Opening dataset {:?}", dsname);
    let ds = client.dataset_by_name(
            &dsname.into(),
            libzfs::DatasetType::Filesystem.into())
        .expect("dataset fail");
    println!("{:?}", ds);
    println!("{:?}", ds.get_name());
    println!("{:?}", ds.get_type());

    for snap in ds.get_filesystems() {
        println!("{:?}", snap.get_name());
    }
}
