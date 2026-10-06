//! Failure injection against the actual patched RMK storage constructor/task.
use embassy_futures::select::{Either, select};
use embedded_storage::nor_flash::{ErrorType, NorFlash, NorFlashErrorKind, ReadNorFlash};
use rmk::{
    config::StorageConfig,
    core_traits::Runnable,
    storage::{Storage, async_flash_wrapper, read_user_data, store_user_data},
    test_support::{InMemoryFlash, test_block_on},
};
type Part = InMemoryFlash<16384, 4096, 1>;
struct PartialErase {
    part: Part,
}
impl ErrorType for PartialErase {
    type Error = NorFlashErrorKind;
}
impl ReadNorFlash for PartialErase {
    const READ_SIZE: usize = 1;
    fn read(&mut self, offset: u32, bytes: &mut [u8]) -> Result<(), Self::Error> {
        self.part.read(offset, bytes)
    }
    fn capacity(&self) -> usize {
        self.part.capacity()
    }
}
impl NorFlash for PartialErase {
    const WRITE_SIZE: usize = 1;
    const ERASE_SIZE: usize = 4096;
    fn write(&mut self, offset: u32, bytes: &[u8]) -> Result<(), Self::Error> {
        self.part.write(offset, bytes)
    }
    fn erase(&mut self, from: u32, to: u32) -> Result<(), Self::Error> {
        self.part.erase(from, (from + 4096).min(to))?;
        Err(NorFlashErrorKind::Other)
    }
}
fn config(id: u8) -> StorageConfig {
    StorageConfig {
        layout_id: Some(u32::from(id)),
        ..Default::default()
    }
}
fn seed(part: Part) {
    test_block_on(async {
        let mut storage =
            Storage::<_, 1, 85, 1, 0>::new(async_flash_wrapper(part), &config(2)).await;
        assert!(matches!(
            select(storage.run(), async {
                store_user_data(0, b"old-layout").await.unwrap();
                assert_eq!(read_user_data(0).await.unwrap().as_slice(), b"old-layout");
            })
            .await,
            Either::Second(())
        ));
    });
}
#[test]
fn failed_schema_write_never_returns_usable_storage() {
    let part = Part::new();
    seed(part.clone());
    part.fail_writes(true);
    let before = part.writes();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        test_block_on(async {
            Storage::<_, 1, 85, 1, 0>::new(async_flash_wrapper(part.clone()), &config(4)).await
        })
    }));
    assert!(
        failure.is_err(),
        "The layout must not start without a successfully written schema"
    );
    assert_eq!(part.writes(), before, "No schema write landed");
    part.fail_writes(false);
    test_block_on(async {
        let mut reopened =
            Storage::<_, 1, 85, 1, 0>::new(async_flash_wrapper(part), &config(4)).await;
        assert!(matches!(
            select(reopened.run(), async {
                assert_eq!(
                    read_user_data(0).await,
                    None,
                    "Fresh layout cannot use pre-erase data"
                );
            })
            .await,
            Either::Second(())
        ));
    });
}
#[test]
fn partial_erase_never_returns_storage_or_stamps_a_schema() {
    let part = Part::new();
    seed(part.clone());
    let before = part.writes();
    let failure = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        test_block_on(async {
            Storage::<_, 1, 85, 1, 0>::new(
                async_flash_wrapper(PartialErase { part: part.clone() }),
                &config(4),
            )
            .await
        })
    }));
    assert!(
        failure.is_err(),
        "Even a partially completed erase must stop initialization"
    );
    assert_eq!(
        part.writes(),
        before,
        "Failed erase must not stamp a new layout marker"
    );
    // A later successful initialization may erase/reset safely; damaged old state
    // cannot be claimed preserved after an erase that physically changed bytes.
    test_block_on(async {
        let mut reopened =
            Storage::<_, 1, 85, 1, 0>::new(async_flash_wrapper(part), &config(4)).await;
        assert!(matches!(
            select(reopened.run(), async {
                assert_eq!(read_user_data(0).await, None);
            })
            .await,
            Either::Second(())
        ));
    });
}
