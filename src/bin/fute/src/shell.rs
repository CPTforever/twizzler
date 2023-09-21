use twizzler_object::ObjID;

use crate::directory::{ open_directory, set_preset_entries, make_dir, get_entry, remove_entry, namei_raw, get_root_id, get_current_id};
use crate::file::File;
use crate::inode::{FileType,  get_inode,   create_inode,  is_directory};
use std::ffi::OsStr;
use std::path::PathBuf;
use human_sort::compare;

pub fn make_root() -> Result<u128, std::io::Error> {
    let (root, id) = create_inode(FileType::Directory)?;

    let dir = open_directory(&root)?;
    set_preset_entries(&dir, id, id);

    Ok(id.as_u128())
}  

pub fn mkdir(path: &str) -> Result<(), std::io::Error> {
    let (root, current) = (get_root_id(), get_current_id());

    let binding = PathBuf::from(path);
    let mut path : Vec<&OsStr> = binding.iter().collect();
    if path.len() == 1 && (path[0] == "/" || path[0] == "." || path[0] == "..") {
        eprintln!("{} already exists", path[0].to_str().unwrap());
    }
    else if path.len() == 0 {
        eprintln!("mkdir needs an operand");
    }

    let file = path.pop().unwrap().to_str().unwrap();

    if path.len() == 0 {
        path.push(OsStr::new("."));
    }

    let node = namei_raw(root, current, path)?;

    make_dir(&node, file);

    Ok(())
}

pub fn ls(path: &str) -> Result<(), std::io::Error> {
    let (root, current) = (get_root_id(), get_current_id());

    let binding = PathBuf::from(path);

    let node = match path == "" {
        true => get_inode(current)?,
        false => {
            let path : Vec<&OsStr> = binding.iter().collect();

            namei_raw(root, current, path)?
        }
    };

    let dir = open_directory(&node)?;
    let top = unsafe {dir.base_unchecked().top};

    
    let mut entries: Vec<(String, String, String)> = vec![];
    for i in 2..top {
        let entry = get_entry(&dir, i).expect("Directory Entry isn't valid");
        if entry.filename.as_bytes() == ".".as_bytes() {continue};
        let inode = get_inode(entry.fileno)?;
        let (filetype, size) = unsafe {
            let x = inode.base_unchecked();
            let filetype = match x.filetype {
                FileType::File => "F".to_owned(),
                FileType::Directory => "D".to_owned(),
            };

            let size = match x.filetype {
                FileType::File => format!("{}", x.size),
                FileType::Directory => "-".to_owned(),
            };

            (filetype, size)
        };

        entries.push((filetype, size, entry.filename.to_string()));
    }

    entries.sort_by(|a, b| compare(&a.2, &b.2));
    if entries.len() > 0 {
        println!("Type\tSize\tName");
    }
    for (filetype, size, filename) in entries {
        println!("{}\t{}\t{}", filetype, size, filename);
    }
    Ok(())
}

pub fn cd(path: &str) -> Result<u128, std::io::Error> {
    let (root, current) = (get_root_id(), get_current_id());
   
    let binding = PathBuf::from(path);
    let path : Vec<&OsStr> = binding.iter().collect();

    let node = namei_raw(root, current, path)?;

    is_directory(&node)?;

    
    Ok(node.id().as_u128())
}

// This causes like major memory leaks lol
pub fn rm(path: &str) -> Result<(), std::io::Error> {
    let (root, current) = (get_root_id(), get_current_id());

    let binding = PathBuf::from(path);
    let mut path : Vec<&OsStr> = binding.iter().collect();
    if path.len() == 1 && (path[0] == "/" || path[0] == "." || path[0] == "..") {
        eprintln!("Refusing to remove {}", path[0].to_str().unwrap());
        return Ok(());
    }
    else if path.len() == 0 {
        eprintln!("mkdir needs an operand");
        return Ok(());
    }

    let file = path.pop().unwrap().to_str().unwrap();

    if path.len() == 0 {
        path.push(OsStr::new("."));
    }

    let node = namei_raw(root, current, path)?;
    let dir = open_directory(&node)?;

    remove_entry(&dir, file)?;
    Ok(())
}


// Traverse down the directory chain and write down names :/
pub fn pwd() -> Result<String, std::io::Error> {
    todo!()
}
