Bạn hãy lên plan cho tôi thực hiện làm 1 bộ tool texture optimizer, phục vụ cho việc tối ưu asset cho Unity/Unreal/Godot như sau.

Các chức năng:
- Resolution fixer: tự động snap các kích thước texture về 4-divisible hoặt POT dimentions, đảm bảo GPU compression tốt
- Sprite/Texture trimmer: loại bỏ các empty transparent pixels. Giảm thiểu các chi tiết vẽ không cần thiết và bounds chặt hơn
- POT padding: safety add padding để đạt được Power-of-Two sizes
- Smart Atlas generator: gomnhoms các texture/sprite thành 1 texture lớn làm atlas, giảm thiểu drawcall. Cho phép hoạt động incremental, nếu kéo thêm hoặc import thêm ảnh thì override kết quả đang có
- Pattern renamer: đổi tên các ảnh theo pattern name tùy ý, ví dụ như smart preix, suffix và auti-incrementing numbers
- Transparent background remover: loại bỏ nền trắng hoặc nền checker của ảnh
- Re-size ảnh: cho phép up/down scale của ảnh
Bonus: tôi muốn thêm chức năng cho texture packing đối với model 3D. Nếu nhiều model 3D sử dụng các texture khác nhau của riêng chúng, có cách nào để pack thành texture to, đảm bảo các model 3D đấy có thể sử dụng không. Texture sau khi pack lại phải là POT sizes để đảm bảo tối ưu.

Yêu cầu:
- là 1 desktop app, không phải web
- chạy được trên cả 3 platform là Windows, macOS và Linux
- hoạt động dựa theo các tab, tối ưu hóa bằng cách sleep các tab đang không sử dụng, chỉ awake tab hiện tại thôi
- UI/UX đẹp, trực quan, dễ nhìn, dễ hiểu
- 1 tab có thể thực hiện với 1 hoặc nhiều ảnh cùng lúc. Có thể kéo thả hoặc chọn 1 hoăc nhiều file, hoặc chọn 1 folder để import. Sau khi import, có 1 placeholder để hiển thị các ảnh được sắp xếp theo hàng lối và cuộn lên xuống được trong các placeholder đấy. Các ảnh trong placeholder hiển thị theo grid, có 3 chế độ hiển hị là big, medium, small size trong placeholder. Đằng cuối có thêm 1 ô dấu + để có thể thêm vào ảnh mới nếu bấm. User cũng có thể kéo thả vào placeholder đấy luôn. Mỗi grid cell đều có badge remove trên góc top right để tiện tay xóa. Có chức năng undo/redo để tránh các thao tác xóa nhầm
- Tất cả các chức năng kể trên, đều phải có đầy đủ các thông số để tôi có thể tùy chỉnh. Có undo/redo để hồi phục lại thông số trước.
- Có localization, 2 ngôn ngữ là English và Tiếng Việt. Các ngôn ngữ cần phải tách riêng thành các file để tôi dễ hiệu đính về sau. Tool chỉ đọc text theo ID chứ không lấy nguyên văn của text truyền vào

Hãy tư vấn tech-stack cho tôi trước khi lên plan. Sau đấy tự tạo file git ignore cho phù hợp với ngôn ngữ lập trình và tech stack đó