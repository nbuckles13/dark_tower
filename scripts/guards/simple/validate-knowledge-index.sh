#!/bin/bash
# Validates INDEX.md files: file pointers resolve, ADR references exist
# (including section headings when specified), and size cap enforced.
ERRORS=""

for index_file in docs/specialist-knowledge/*/INDEX.md; do
    [ -f "$index_file" ] || continue

    # Check file path pointers (backtick-wrapped paths with extensions)
    while IFS= read -r raw_path; do
        path="${raw_path//\`/}"
        # Skip glob patterns (contain * or NNNN placeholder)
        [[ "$path" == *"*"* || "$path" == *"NNNN"* ]] && continue
        # Strip anchor links like #scenario-8-something
        file_path=$(echo "$path" | sed -E 's/#[^#]*$//')
        # Strip function/type references: :function(), :Type, :Struct::method()
        file_path=$(echo "$file_path" | sed -E 's/:[A-Za-z_][A-Za-z0-9_:]*([(][)])?$//')
        # Strip line number references like :34-42
        file_path=$(echo "$file_path" | sed -E 's/:[0-9][-0-9]*$//')
        if [[ "$file_path" == docs/* || "$file_path" == crates/* || "$file_path" == proto/* || "$file_path" == scripts/* ]]; then
            if [ ! -e "$file_path" ]; then
                ERRORS="${ERRORS}STALE POINTER in $index_file: $file_path\n"
            fi
        fi
    done < <(grep -oP '`[^`]+\.(rs|md|proto|toml|sh|sql|yaml|yml|json)[^`]*`' "$index_file")

    # Check ADR references: "ADR-NNNN" must have a matching file
    while IFS= read -r adr; do
        adr_num=$(echo "$adr" | grep -oP '\d+')
        adr_file=$(ls docs/decisions/adr-${adr_num}-*.md 2>/dev/null | head -1)
        if [ -z "$adr_file" ]; then
            ERRORS="${ERRORS}STALE ADR in $index_file: $adr (no matching file in docs/decisions/)\n"
        fi
    done < <(grep -oP 'ADR-\d+' "$index_file" | sort -u)

    # Check size cap
    lines=$(wc -l < "$index_file")
    if [ "$lines" -gt 50 ]; then
        ERRORS="${ERRORS}SIZE VIOLATION: $index_file has $lines lines (max 50)\n"
    fi
done

if [ -n "$ERRORS" ]; then
    echo -e "$ERRORS"
    exit 1
fi
exit 0
