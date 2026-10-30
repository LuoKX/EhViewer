"""Independent production-owner gates for the existing application's next Rust slices."""
try:
    from .check_p3_coverage import main
except ImportError:
    from check_p3_coverage import main

OWNERS = (
    ("eh_domain", "recovery_persistence", "crates/domain/src/recovery_persistence.rs"),
    ("eh_domain", "reader_navigation", "crates/domain/src/reader_navigation.rs"),
    ("eh_domain", "thumbnail_flow", "crates/domain/src/thumbnail_flow.rs"),
    ("eh_domain", "quick_search", "crates/domain/src/quick_search.rs"),
    ("eh_domain", "list_order", "crates/domain/src/list_order.rs"),
    ("eh_domain", "favorite_batch", "crates/domain/src/favorite_batch.rs"),
    ("eh_domain", "download_startup", "crates/domain/src/download_startup.rs"),
    ("eh_domain", "catalog_state", "crates/domain/src/catalog_state.rs"),
    ("eh_domain", "label_rules", "crates/domain/src/label_rules.rs"),
    ("eh_domain", "entry_actions", "crates/domain/src/entry_actions.rs"),
    ("eh_domain", "recovery_mapping", "crates/domain/src/recovery_mapping.rs"),
    ("eh_domain", "download_files", "crates/domain/src/download_files.rs"),
    ("eh_archive", "update_package", "crates/archive/src/update_package.rs"),
    ("eh_image", "crop_policy", "crates/image/src/crop_policy.rs"),
    ("eh_domain", "comment_display", "crates/domain/src/comment_display.rs"),
)

if __name__ == "__main__":
    main(OWNERS, "followup")
