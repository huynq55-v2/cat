#!/bin/bash

# Kiểm tra thư mục mục tiêu (mặc định là thư mục hiện tại)
TARGET_DIR=${1:-.}

echo "🚀 Đang làm sạch mã nguồn Rust trong: $TARGET_DIR"

# Sử dụng find để quét các file .rs
find "$TARGET_DIR" -type f -name "*.rs" | while read -r file; do
    # 1. Xóa comment // 
    # 2. Xóa khoảng trắng cuối dòng (trailing whitespaces)
    # 3. Gộp nhiều dòng trống liên tiếp thành 1 dòng trống duy nhất (dùng lệnh cat -s hoặc sed)
    
    # Cách dùng sed để xử lý tối ưu:
    sed -i -E '
        /^[[:space:]]*\/\//d;       # Xóa dòng chỉ có comment //
        s/\/\/[^"]*//g;             # Xóa comment // đứng sau code
        s/[[:space:]]+$//;          # Xóa khoảng trắng thừa ở cuối mỗi dòng
    ' "$file"

    # Sử dụng cat -s để ép các dòng trống chồng lên nhau thành 1 dòng duy nhất
    # Sau đó ghi đè lại vào file
    cat -s "$file" > "$file.tmp" && mv "$file.tmp" "$file"
done

echo "✅ Hoàn tất! Đã xóa comment và thu gọn các dòng trống dư thừa."