"""Keep Usage data and remove every other table from the main SQLite database.

The application recreates the removed tables on the next startup.
"""

from __future__ import annotations

import argparse
import sqlite3
from datetime import datetime
from pathlib import Path


KEEP_TABLES = {
    "schema_migrations",
    "usage_metadata",
    "usage_logs",
    "usage_sessions",
    "usage_request_records",
    "skill_usage_session_records",
    "usage_pricing_config",
    "usage_pricing_items",
    "codex_quota_stages",
    "provider_key_usage",
    "provider_key_bindings",
}


def quote_identifier(value: str) -> str:
    return '"' + value.replace('"', '""') + '"'


def list_tables(connection: sqlite3.Connection) -> list[str]:
    rows = connection.execute(
        "SELECT name FROM sqlite_master "
        "WHERE type = 'table' AND name NOT LIKE 'sqlite_%' ORDER BY name"
    )
    return [row[0] for row in rows]


def create_backup(database: Path, backup_path: Path) -> None:
    backup_path.parent.mkdir(parents=True, exist_ok=True)
    source = sqlite3.connect(database)
    try:
        target = sqlite3.connect(backup_path)
        try:
            source.backup(target)
        finally:
            target.close()
    finally:
        source.close()


def reset_database(database: Path, dry_run: bool) -> tuple[list[str], Path | None]:
    if not database.is_file():
        raise FileNotFoundError(f"数据库不存在: {database}")

    connection = sqlite3.connect(database)
    try:
        tables = list_tables(connection)
        dropped = [table for table in tables if table not in KEEP_TABLES]
        if dry_run:
            return dropped, None

        stamp = datetime.now().strftime("%Y%m%d-%H%M%S")
        backup_path = database.with_name(f"{database.stem}.before-reset-{stamp}.db")
        create_backup(database, backup_path)

        connection.execute("PRAGMA foreign_keys = OFF")
        connection.execute("BEGIN IMMEDIATE")
        try:
            for table in dropped:
                connection.execute(f"DROP TABLE IF EXISTS {quote_identifier(table)}")
            connection.commit()
        except Exception:
            connection.rollback()
            raise

        connection.execute("PRAGMA wal_checkpoint(TRUNCATE)")
        connection.execute("VACUUM")
        integrity = connection.execute("PRAGMA integrity_check").fetchone()[0]
        if integrity != "ok":
            raise RuntimeError(f"数据库完整性检查失败: {integrity}")
        return dropped, backup_path
    finally:
        connection.close()


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--database",
        type=Path,
        default=Path(r"D:\ai-manager-data\workspace\storage\ai-manager.db"),
        help="主数据库路径",
    )
    parser.add_argument(
        "--dry-run",
        action="store_true",
        help="只显示将删除的表，不修改数据库",
    )
    args = parser.parse_args()

    dropped, backup_path = reset_database(args.database, args.dry_run)
    if args.dry_run:
        print("将删除以下表:")
        for table in dropped:
            print(f"  - {table}")
        print("保留表:")
        for table in sorted(KEEP_TABLES):
            print(f"  - {table}")
        return 0

    print(f"已删除 {len(dropped)} 个非 Usage 表。")
    print(f"备份文件: {backup_path}")
    print("请启动应用，让当前项目代码重新创建其他表。")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
