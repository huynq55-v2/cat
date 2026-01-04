#!/bin/bash

# Kiểm tra xem thư mục mục tiêu có được cung cấp không, mặc định là thư mục hiện tại
TARGET_DIR=${1:-.}

echo "Đang quét các file .rs trong: $TARGET_DIR"

# Tìm tất cả các file có đuôi .rs và thực hiện xóa comment
find "$TARGET_DIR" -type f -name "*.rs" -exec sed -i '/^[[:space:]]*\/\//d; s/\/\/[^"]*//g' {} +

echo "Hoàn thành! Tất cả comment dạng // đã được loại bỏ."