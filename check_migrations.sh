#!/bin/bash

# Paths
MIGRATION_DIR="src-tauri/db_migrations/"
DATABASE_RS="src-tauri/src/database.rs"

# Extract migration filenames from database.rs
echo "Extracting expected migration files from database.rs..."
expected_files=()
while read -r line; do
    # The line should look like: "0001_initial.sql",
    if [[ $line =~ \"([0-9]{4}_[^\"]+\.sql)\" ]]; then
        filename="${BASH_REMATCH[1]}"
        expected_files+=("$filename")
    fi
done < <(grep -A 50 "const MIGRATION_FILES" "$DATABASE_RS" | grep -B 50 "Add new migrations" | grep -E "\"[0-9]{4}_.*\.sql\"")

# Get list of actual migration files from the directory
actual_files=()
while IFS= read -r line; do
    actual_files+=("$line")
done < <(ls "$MIGRATION_DIR")

# Debug output
echo "Found ${#expected_files[@]} expected migrations in database.rs:"
for file in "${expected_files[@]}"; do
    echo "  - $file"
done

echo -e "\nFound ${#actual_files[@]} actual migration files in $MIGRATION_DIR:"
for file in "${actual_files[@]}"; do
    echo "  - $file"
done

# Check if all expected files exist in the directory
echo -e "\nChecking that all expected files exist..."
missing_files=0
for expected in "${expected_files[@]}"; do
    found=false
    for actual in "${actual_files[@]}"; do
        if [[ "$expected" == "$actual" ]]; then
            found=true
            break
        fi
    done

    if [[ "$found" == false ]]; then
        echo "ERROR: Missing migration file: $expected"
        missing_files=$((missing_files + 1))
    fi
done

# Check if all actual files are expected
echo -e "\nChecking that no unexpected files exist..."
unexpected_files=0
for actual in "${actual_files[@]}"; do
    found=false
    for expected in "${expected_files[@]}"; do
        if [[ "$actual" == "$expected" ]]; then
            found=true
            break
        fi
    done

    if [[ "$found" == false ]]; then
        echo "ERROR: Unexpected migration file found: $actual"
        echo "This file exists but is not referenced in database.rs"
        unexpected_files=$((unexpected_files + 1))
    fi
done

# Final result
if [[ $missing_files -gt 0 || $unexpected_files -gt 0 ]]; then
    echo -e "\nFound $missing_files missing files and $unexpected_files unexpected files."
    exit 1
else
    echo -e "\nAll migration files are correctly referenced in database.rs and exist in the directory."
fi
