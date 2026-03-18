use super::{
    block_cache_sync_all, get_block_cache, BlockDevice, DirEntry, DiskInode, DiskInodeType,
    EasyFileSystem, DIRENT_SZ,
};
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec::Vec;
use spin::{Mutex, MutexGuard};
/// Virtual filesystem layer over easy-fs
pub struct Inode {
    /// 当前 inode 的编号，直接存储以避免死锁
    inode_id: u32,
    block_id: usize,
    block_offset: usize,
    fs: Arc<Mutex<EasyFileSystem>>,
    block_device: Arc<dyn BlockDevice>,
}

impl Inode {
    /// Create a vfs inode
    pub fn new(
        inode_id: u32,
        block_id: u32,
        block_offset: usize,
        fs: Arc<Mutex<EasyFileSystem>>,
        block_device: Arc<dyn BlockDevice>,
    ) -> Self {
        Self {
            inode_id,
            block_id: block_id as usize,
            block_offset,
            fs,
            block_device,
        }
    }

    /// Call a function over a disk inode to read it
    fn read_disk_inode<V>(&self, f: impl FnOnce(&DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .read(self.block_offset, f)
    }

    /// Call a function over a disk inode to modify it
    fn modify_disk_inode<V>(&self, f: impl FnOnce(&mut DiskInode) -> V) -> V {
        get_block_cache(self.block_id, Arc::clone(&self.block_device))
            .lock()
            .modify(self.block_offset, f)
    }

    /// Find inode under a disk inode by name
    fn find_inode_id(&self, name: &str, disk_inode: &DiskInode) -> Option<u32> {
        // assert it is a directory
        assert!(disk_inode.is_dir());
        let file_count = (disk_inode.size as usize) / DIRENT_SZ;
        let mut dirent = DirEntry::empty();
        for i in 0..file_count {
            assert_eq!(
                disk_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device,),
                DIRENT_SZ,
            );
            if dirent.name() == name {
                return Some(dirent.inode_number());
            }
        }
        None
    }

    /// Find inode under current inode by name
    pub fn find(&self, name: &str) -> Option<Arc<Inode>> {
        // 目录查找流程：目录 inode -> 遍历 dirent -> 定位子 inode 的磁盘位置。
        let fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode).map(|inode_id| {
                let (block_id, block_offset) = fs.get_disk_inode_pos(inode_id);
                Arc::new(Self::new(
                    inode_id,
                    block_id,
                    block_offset,
                    self.fs.clone(),
                    self.block_device.clone(),
                ))
            })
        })
    }

    /// Increase the size of a disk inode
    fn increase_size(
        &self,
        new_size: u32,
        disk_inode: &mut DiskInode,
        fs: &mut MutexGuard<EasyFileSystem>,
    ) {
        if new_size < disk_inode.size {
            return;
        }
        // 先按"新增块数"批量申请数据块，再一次性扩容 inode。
        let blocks_needed = disk_inode.blocks_num_needed(new_size);
        let mut v: Vec<u32> = Vec::new();
        for _ in 0..blocks_needed {
            v.push(fs.alloc_data());
        }
        disk_inode.increase_size(new_size, v, &self.block_device);
    }

    /// Create inode under current inode by name.
    /// Attention: use find previously to ensure the new file not existing.
    pub fn create(&self, name: &str) -> Option<Arc<Inode>> {
        let mut fs = self.fs.lock();
        // 1) 分配新 inode
        let new_inode_id = fs.alloc_inode();
        // 2) 初始化 inode 元数据
        let (new_inode_block_id, new_inode_block_offset) = fs.get_disk_inode_pos(new_inode_id);
        get_block_cache(new_inode_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(new_inode_block_offset, |new_inode: &mut DiskInode| {
                new_inode.initialize(DiskInodeType::File);
            });
        // 3) 在当前目录追加 dirent 项
        self.modify_disk_inode(|root_inode| {
            // append file in the dirent
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            // increase size
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // write dirent
            let dirent = DirEntry::new(name, new_inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });

        let (block_id, block_offset) = fs.get_disk_inode_pos(new_inode_id);
        block_cache_sync_all();
        // 4) 返回新文件的 Inode 句柄
        Some(Arc::new(Self::new(
            new_inode_id,
            block_id,
            block_offset,
            self.fs.clone(),
            self.block_device.clone(),
        )))
        // release efs lock automatically by compiler
    }

    /// List inodes by id under current inode
    pub fn readdir(&self) -> Vec<String> {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| {
            let file_count = (disk_inode.size as usize) / DIRENT_SZ;
            let mut v: Vec<String> = Vec::new();
            for i in 0..file_count {
                let mut dirent = DirEntry::empty();
                assert_eq!(
                    disk_inode.read_at(i * DIRENT_SZ, dirent.as_bytes_mut(), &self.block_device,),
                    DIRENT_SZ,
                );
                v.push(String::from(dirent.name()));
            }
            v
        })
    }

    /// Read data from current inode
    pub fn read_at(&self, offset: usize, buf: &mut [u8]) -> usize {
        let _fs = self.fs.lock();
        self.read_disk_inode(|disk_inode| disk_inode.read_at(offset, buf, &self.block_device))
    }

    /// Write data to current inode
    pub fn write_at(&self, offset: usize, buf: &[u8]) -> usize {
        let mut fs = self.fs.lock();
        let size = self.modify_disk_inode(|disk_inode| {
            self.increase_size((offset + buf.len()) as u32, disk_inode, &mut fs);
            disk_inode.write_at(offset, buf, &self.block_device)
        });
        block_cache_sync_all();
        size
    }

    /// Clear the data in current inode
    pub fn clear(&self) {
        let mut fs = self.fs.lock();
        self.modify_disk_inode(|disk_inode| {
            let size = disk_inode.size;
            let data_blocks_dealloc = disk_inode.clear_size(&self.block_device);
            assert!(data_blocks_dealloc.len() == DiskInode::total_blocks(size) as usize);
            for data_block in data_blocks_dealloc.into_iter() {
                fs.dealloc_data(data_block);
            }
        });
        block_cache_sync_all();
    }
    
    /// 获取硬链接计数
    pub fn nlink(&self) -> u32 {
        self.read_disk_inode(|disk_inode| disk_inode.nlink())
    }
    
    /// 检查是否是目录
    pub fn is_dir(&self) -> bool {
        self.read_disk_inode(|disk_inode| disk_inode.is_dir())
    }
    
    /// 增加硬链接计数
    pub fn inc_nlink(&self) {
        self.modify_disk_inode(|disk_inode| {
            disk_inode.inc_nlink();
        });
        block_cache_sync_all();
    }
    
    /// 减少硬链接计数
    pub fn dec_nlink(&self) -> u32 {
        let nlink = self.modify_disk_inode(|disk_inode| {
            disk_inode.dec_nlink()
        });
        block_cache_sync_all();
        nlink
    }
    
    /// 创建硬链接：为当前文件创建一个新名称
    pub fn link(&self, name: &str, old_inode: &Arc<Inode>) -> isize {
        // 检查新文件名是否已存在
        if self.find(name).is_some() {
            return -1;
        }
        
        let mut fs = self.fs.lock();
        
        // 增加原文件的硬链接计数
        old_inode.modify_disk_inode(|disk_inode| {
            disk_inode.inc_nlink();
        });
        
        // 在当前目录中添加新的目录项
        self.modify_disk_inode(|root_inode| {
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let new_size = (file_count + 1) * DIRENT_SZ;
            self.increase_size(new_size as u32, root_inode, &mut fs);
            // 直接使用 old_inode.inode_id，不再加锁计算
            let dirent = DirEntry::new(name, old_inode.inode_id);
            root_inode.write_at(
                file_count * DIRENT_SZ,
                dirent.as_bytes(),
                &self.block_device,
            );
        });
        block_cache_sync_all();
        0
    }
    
    /// 删除目录项（由 unlink 调用）
    pub fn unlink(&self, name: &str) -> isize {
        let mut fs = self.fs.lock();
        
        // 获取目标 inode 编号
        let target_inode_id = self.read_disk_inode(|disk_inode| {
            self.find_inode_id(name, disk_inode)
        });
        
        let target_inode_id = match target_inode_id {
            Some(id) => id,
            None => return -1,
        };
        
        // 获取目标 inode 的位置
        let (target_block_id, target_block_offset) = fs.get_disk_inode_pos(target_inode_id);
        
        // 查找并删除目录项
        let mut found = false;
        self.modify_disk_inode(|root_inode| {
            let file_count = (root_inode.size as usize) / DIRENT_SZ;
            let mut dirent = DirEntry::empty();
            for i in 0..file_count {
                assert_eq!(
                    root_inode.read_at(DIRENT_SZ * i, dirent.as_bytes_mut(), &self.block_device),
                    DIRENT_SZ,
                );
                if dirent.name() == name {
                    // 找到目标目录项
                    found = true;
                    // 将最后一个目录项移到当前位置
                    if i < file_count - 1 {
                        let last_offset = (file_count - 1) * DIRENT_SZ;
                        assert_eq!(
                            root_inode.read_at(last_offset, dirent.as_bytes_mut(), &self.block_device),
                            DIRENT_SZ,
                        );
                        root_inode.write_at(
                            i * DIRENT_SZ,
                            dirent.as_bytes(),
                            &self.block_device,
                        );
                    }
                    // 手动缩小目录大小（increase_size 不支持缩小）
                    root_inode.size = ((file_count - 1) * DIRENT_SZ) as u32;
                    break;
                }
            }
        });
        
        if !found {
            return -1;
        }
        
        // 减少目标 inode 的硬链接计数
        let nlink = get_block_cache(target_block_id as usize, Arc::clone(&self.block_device))
            .lock()
            .modify(target_block_offset, |disk_inode: &mut DiskInode| {
                disk_inode.dec_nlink()
            });
        
        // 如果硬链接计数为 0，回收 inode 和数据块
        if nlink == 0 {
            // 获取目标 inode 的信息并清空
            let (target_block_id, target_block_offset) = fs.get_disk_inode_pos(target_inode_id);
            let data_blocks = get_block_cache(target_block_id as usize, Arc::clone(&self.block_device))
                .lock()
                .modify(target_block_offset, |disk_inode: &mut DiskInode| {
                    disk_inode.clear_size(&self.block_device)
                });
            
            // 回收数据块
            for data_block in data_blocks.into_iter() {
                fs.dealloc_data(data_block);
            }
            
            // 回收 inode（在 inode 位图中释放）
            fs.dealloc_inode(target_inode_id);
        }
        
        block_cache_sync_all();
        0
    }
    
    /// 获取当前 inode 编号
    /// 直接返回存储的 inode_id，不再加锁计算，避免死锁
    pub fn inode_number(&self) -> u32 {
        self.inode_id
    }
}
