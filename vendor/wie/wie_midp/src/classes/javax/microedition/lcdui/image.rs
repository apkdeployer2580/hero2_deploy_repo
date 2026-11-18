use alloc::{borrow::Cow, boxed::Box, vec, vec::Vec};
use core::marker::PhantomData;

use bytemuck::cast_vec;

use java_class_proto::{JavaFieldProto, JavaMethodProto};
use java_constants::MethodAccessFlags;
use java_runtime::classes::java::lang::String;
use jvm::{
    Array, ArrayRawBufferMut, ClassInstanceRef, Jvm, Result as JvmResult,
    runtime::{JavaIoInputStream, JavaLangClassLoader, JavaLangString},
};

use wie_backend::canvas::{
    ArgbPixel, Canvas, Color, Image as BackendImage, ImageBuffer, ImageBufferCanvas, PixelType, Rgb332Pixel, Rgb565Pixel, decode_image,
};
use wie_jvm_support::{WieJavaClassProto, WieJvmContext};

use crate::classes::javax::microedition::lcdui::Graphics;

// class javax.microedition.lcdui.Image
pub struct Image;

impl Image {
    pub fn as_proto() -> WieJavaClassProto {
        WieJavaClassProto {
            name: "javax/microedition/lcdui/Image",
            parent_class: Some("java/lang/Object"),
            interfaces: vec![],
            methods: vec![
                JavaMethodProto::new("<init>", "(II[BI)V", Self::init, Default::default()),
                JavaMethodProto::new("getWidth", "()I", Self::get_width, Default::default()),
                JavaMethodProto::new("getHeight", "()I", Self::get_height, Default::default()),
                JavaMethodProto::new(
                    "getGraphics",
                    "()Ljavax/microedition/lcdui/Graphics;",
                    Self::get_graphics,
                    Default::default(),
                ),
                JavaMethodProto::new(
                    "createImage",
                    "(II)Ljavax/microedition/lcdui/Image;",
                    Self::create_image,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "createImage",
                    "([BII)Ljavax/microedition/lcdui/Image;",
                    Self::create_image_from_data,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "createImage",
                    "(Ljava/lang/String;)Ljavax/microedition/lcdui/Image;",
                    Self::create_image_from_name,
                    MethodAccessFlags::STATIC,
                ),
                JavaMethodProto::new(
                    "createImage",
                    "(Ljavax/microedition/lcdui/Image;)Ljavax/microedition/lcdui/Image;",
                    Self::create_image_from_image,
                    MethodAccessFlags::STATIC,
                ),
            ],
            fields: vec![
                JavaFieldProto::new("w", "I", Default::default()),
                JavaFieldProto::new("h", "I", Default::default()),
                JavaFieldProto::new("imgData", "[B", Default::default()),
                JavaFieldProto::new("bpl", "I", Default::default()),
            ],
            access_flags: Default::default(),
        }
    }

    async fn init(
        jvm: &Jvm,
        _context: &mut WieJvmContext,
        mut this: ClassInstanceRef<Self>,
        width: i32,
        height: i32,
        img_data: ClassInstanceRef<Array<i8>>,
        bpl: i32,
    ) -> JvmResult<()> {
        tracing::debug!("javax.microedition.lcdui.Image::<init>({this:?}, {width}, {height}, {img_data:?}, {bpl})");

        let _: () = jvm.invoke_special(&this, "java/lang/Object", "<init>", "()V", ()).await?;

        jvm.put_field(&mut this, "w", "I", width).await?;
        jvm.put_field(&mut this, "h", "I", height).await?;
        jvm.put_field(&mut this, "imgData", "[B", img_data).await?;
        jvm.put_field(&mut this, "bpl", "I", bpl).await?;

        Ok(())
    }

    async fn create_image(jvm: &Jvm, _: &mut WieJvmContext, width: i32, height: i32) -> JvmResult<ClassInstanceRef<Image>> {
        tracing::debug!("javax.microedition.lcdui.Image::createImage({width}, {height})");

        let bytes_per_pixel = 4;

        Self::create_image_instance(
            jvm,
            width as _,
            height as _,
            &vec![0; (width * height * bytes_per_pixel) as usize],
            bytes_per_pixel as _,
        )
        .await
    }

    async fn create_image_from_name(jvm: &Jvm, _: &mut WieJvmContext, name: ClassInstanceRef<String>) -> JvmResult<ClassInstanceRef<Image>> {
        tracing::debug!("javax.microedition.lcdui.Image::createImage({name:?})");

        let name = JavaLangString::to_rust_string(jvm, &name).await?;

        let class_loader = jvm.current_class_loader().await?;
        let stream = JavaLangClassLoader::get_resource_as_stream(jvm, &class_loader, &name).await?.unwrap();

        let image_data = JavaIoInputStream::read_until_end(jvm, &stream).await?;
        let image_data_len = image_data.len() as i32;

        let mut image_array = jvm.instantiate_array("B", image_data_len as _).await?;
        jvm.array_raw_buffer_mut(&mut image_array).await?.write(0, &image_data)?;

        jvm.invoke_static(
            "javax/microedition/lcdui/Image",
            "createImage",
            "([BII)Ljavax/microedition/lcdui/Image;",
            (image_array, 0, image_data_len),
        )
        .await
    }

    async fn create_image_from_data(
        jvm: &Jvm,
        _: &mut WieJvmContext,
        data: ClassInstanceRef<Array<i8>>,
        image_offset: i32,
        image_length: i32,
    ) -> JvmResult<ClassInstanceRef<Image>> {
        tracing::debug!("javax.microedition.lcdui.Image::createImage({data:?}, {image_offset}, {image_length})");

        let mut image_data = vec![0; image_length as usize];
        jvm.array_raw_buffer(&data).await?.read(image_offset as _, &mut image_data)?;

        let image = {
            let result = decode_image(&cast_vec(image_data));
            if let Ok(image) = result {
                image
            } else {
                tracing::error!("Failed to decode image: {:?}", result.err());
                let exception = jvm.exception("java/lang/IllegalArgumentException", "Failed to decode image").await;

                return Err(exception);
            }
        };

        Self::create_image_instance(jvm, image.width(), image.height(), &image.raw(), image.bytes_per_pixel()).await
    }

    async fn create_image_from_image(jvm: &Jvm, _: &mut WieJvmContext, image: ClassInstanceRef<Image>) -> JvmResult<ClassInstanceRef<Image>> {
        tracing::debug!("javax.microedition.lcdui.Image::createImage({image:?})");

        let src_image = Image::image(jvm, &image).await?;

        Self::create_image_instance(jvm, src_image.width(), src_image.height(), &src_image.raw(), src_image.bytes_per_pixel()).await
    }

    async fn get_graphics(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<ClassInstanceRef<Graphics>> {
        tracing::debug!("javax.microedition.lcdui.Image::getGraphics({this:?})");

        let instance = jvm
            .new_class(
                "javax/microedition/lcdui/Graphics",
                "(Ljavax/microedition/lcdui/Image;)V",
                (this.clone(),),
            )
            .await?;

        Ok(instance.into())
    }

    async fn get_width(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.lcdui.Image::getWidth({this:?})");

        jvm.get_field(&this, "w", "I").await
    }

    async fn get_height(jvm: &Jvm, _: &mut WieJvmContext, this: ClassInstanceRef<Self>) -> JvmResult<i32> {
        tracing::debug!("javax.microedition.lcdui.Image::getHeight({this:?})");

        jvm.get_field(&this, "h", "I").await
    }

    pub async fn image(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> JvmResult<Box<dyn BackendImage>> {
        let width: i32 = jvm.get_field(this, "w", "I").await?;
        let bpl: i32 = jvm.get_field(this, "bpl", "I").await?;

        let bytes_per_pixel = bpl / width;

        Ok(match bytes_per_pixel {
            1 => Box::new(JavaImageBuffer::<Rgb332Pixel>::new(jvm, this).await?) as _,
            2 => Box::new(JavaImageBuffer::<Rgb565Pixel>::new(jvm, this).await?) as _,
            4 => Box::new(JavaImageBuffer::<ArgbPixel>::new(jvm, this).await?) as _,
            _ => unimplemented!("Unsupported pixel format: {bytes_per_pixel}"),
        })
    }

    pub async fn canvas(jvm: &Jvm, this: &ClassInstanceRef<Self>) -> JvmResult<Box<dyn Canvas>> {
        let width: i32 = jvm.get_field(this, "w", "I").await?;
        let bpl: i32 = jvm.get_field(this, "bpl", "I").await?;

        let bytes_per_pixel = bpl / width;

        Ok(match bytes_per_pixel {
            1 => Box::new(ImageBufferCanvas::new(JavaImageBuffer::<Rgb332Pixel>::new(jvm, this).await?)) as _,
            2 => Box::new(ImageBufferCanvas::new(JavaImageBuffer::<Rgb565Pixel>::new(jvm, this).await?)) as _,
            4 => Box::new(ImageBufferCanvas::new(JavaImageBuffer::<ArgbPixel>::new(jvm, this).await?)) as _,
            _ => unimplemented!("Unsupported pixel format: {bytes_per_pixel}"),
        })
    }

    async fn create_image_instance(jvm: &Jvm, width: u32, height: u32, data: &[u8], bytes_per_pixel: u32) -> JvmResult<ClassInstanceRef<Image>> {
        let mut data_array = jvm.instantiate_array("B", data.len() as _).await?;
        jvm.array_raw_buffer_mut(&mut data_array).await?.write(0, data)?;

        Ok(jvm
            .new_class(
                "javax/microedition/lcdui/Image",
                "(II[BI)V",
                (width as i32, height as i32, data_array, (width * bytes_per_pixel) as i32),
            )
            .await?
            .into())
    }
}

struct JavaImageBuffer<T>
where
    T: PixelType,
{
    width: i32,
    height: i32,
    raw_buffer: Box<dyn ArrayRawBufferMut>,
    _phantom: PhantomData<T>,
}

impl<T> JavaImageBuffer<T>
where
    T: PixelType,
{
    pub async fn new(jvm: &Jvm, this: &ClassInstanceRef<Image>) -> JvmResult<Self> {
        let mut java_img_data = jvm.get_field(this, "imgData", "[B").await?;
        let raw_buffer = jvm.array_raw_buffer_mut(&mut java_img_data).await?;

        let width: i32 = jvm.get_field(this, "w", "I").await?;
        let height: i32 = jvm.get_field(this, "h", "I").await?;

        Ok(Self {
            width,
            height,
            raw_buffer,
            _phantom: PhantomData,
        })
    }
}

impl<T> BackendImage for JavaImageBuffer<T>
where
    T: PixelType,
{
    fn width(&self) -> u32 {
        self.width as _
    }

    fn height(&self) -> u32 {
        self.height as _
    }

    fn bytes_per_pixel(&self) -> u32 {
        size_of::<T::DataType>() as _
    }

    fn get_pixel(&self, x: i32, y: i32) -> Color {
        let offset = (((y as u32) * self.width() + (x as u32)) * self.bytes_per_pixel()) as usize;

        // ★ 2026-09-08 (순서표 #8): 여기 있던 `vec![0; bpp]` 는 **픽셀당 힙 할당**이었다.
        // hero2 실측으로 스프라이트 draw 가 그려진 프레임마다 24,232 픽셀을 지나가므로
        // (`[canvas]` 게이지), 그 한 줄이 프레임당 2만 번 이상의 malloc/free 였다.
        // 픽셀 하나는 최대 4바이트(`ArgbPixel`)이므로 스택 배열로 충분하다.
        let n = size_of::<T::DataType>();
        let mut buffer = [0u8; 4];
        self.raw_buffer.read(offset as _, &mut buffer[..n]).unwrap();

        T::to_color(*bytemuck::from_bytes(&buffer[..n]))
    }

    fn raw(&self) -> Cow<'_, [u8]> {
        let size = self.width() * self.height() * self.bytes_per_pixel();
        let mut buffer = vec![0; size as usize];
        self.raw_buffer.read(0, &mut buffer).unwrap();

        Cow::Owned(buffer)
    }

    fn colors(&self) -> Vec<Color> {
        let buffer = self.raw();

        buffer
            .chunks_exact(size_of::<T::DataType>())
            .map(|chunk| T::to_color(*bytemuck::from_bytes(chunk)))
            .collect()
    }

    /// One bulk read out of the guest array instead of one per pixel. See
    /// `Image::raw_row` for why that difference is the whole point.
    fn raw_row(&self, x: i32, y: i32, px: usize, out: &mut [u8]) -> bool {
        let bpp = self.bytes_per_pixel() as usize;
        if x < 0 || y < 0 || out.len() != px * bpp {
            return false;
        }
        if (y as u32) >= self.height() || (x as u32).saturating_add(px as u32) > self.width() {
            return false;
        }
        let offset = ((y as u32) * self.width() + (x as u32)) as usize * bpp;

        self.raw_buffer.read(offset as _, out).is_ok()
    }
}

impl<T> ImageBuffer for JavaImageBuffer<T>
where
    T: PixelType,
{
    fn put_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x > self.width || y > self.height || x < 0 || y < 0 {
            return;
        }

        let offset = (((y as u32) * self.width() + (x as u32)) * self.bytes_per_pixel()) as usize;

        let raw = T::from_color(color);
        let raw_bytes = bytemuck::bytes_of(&raw);

        self.raw_buffer.write(offset as _, raw_bytes).unwrap();
    }

    fn put_pixels(&mut self, x: i32, y: i32, _width: u32, colors: &[Color]) {
        if x > self.width || y > self.height || x < 0 || y < 0 {
            return;
        }

        let offset = (((y as u32) * self.width() + (x as u32)) * self.bytes_per_pixel()) as usize;

        let raw_bytes = colors
            .iter()
            .flat_map(|color| bytemuck::bytes_of(&T::from_color(*color)).to_vec())
            .collect::<Vec<_>>();

        self.raw_buffer.write(offset as _, &raw_bytes).unwrap();
    }

    /// ★ 2026-09-08 (순서표 #8). 이게 없으면 `Canvas::fill_rect` 가 트레이트 기본 구현으로
    /// 떨어지고, 그건 픽셀당 `put_pixel` 이다 — 즉 픽셀마다 경계검사 + 색 변환 + **게스트
    /// ARM 메모리로의 dyn 디스패치 쓰기** 한 번씩.
    ///
    /// hero2 실측(`[canvas]` 게이지): 전체화면 지우기가 **호출 한 번에 30,134 픽셀**
    /// (176x220 의 0.78 화면분)을, 그려진 프레임마다 통과했다. 그 3만 번의 쓰기를 행 단위
    /// 220번으로 바꾼다 — 색 변환은 픽셀당이 아니라 **한 번**, 행 버퍼는 호출당 하나다.
    ///
    /// (짝인 `put_row_raw` 는 일부러 구현하지 않는다: `canvas.rs` 의 raw-row 경로는
    /// src·dst 가 **둘 다 2 B/px** 일 때만 열리는데 이 게임은 4/4 라 도달 자체가 불가능하다.
    /// 구현해도 한 번도 안 불린다 — 픽셀 포맷을 먼저 바꾸기 전에는 무의미하다.)
    fn fill_span(&mut self, x0: i32, y0: i32, x1: i32, y1: i32, color: Color) {
        // 호출자(`Canvas::fill_rect`)가 이미 클램프하지만, 트레이트 기본 구현은 `put_pixel`
        // 의 자체 검사에 기대고 있었다. 그 안전망이 사라지므로 여기서 다시 좁힌다.
        let x0 = x0.max(0);
        let y0 = y0.max(0);
        let x1 = x1.min(self.width);
        let y1 = y1.min(self.height);
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let bpp = self.bytes_per_pixel() as usize;
        let raw = T::from_color(color);
        let pattern = bytemuck::bytes_of(&raw);

        let row_px = (x1 - x0) as usize;
        let mut row = vec![0u8; row_px * bpp];
        for cell in row.chunks_exact_mut(bpp) {
            cell.copy_from_slice(pattern);
        }

        for y in y0..y1 {
            let offset = (((y as u32) * self.width() + (x0 as u32)) * bpp as u32) as usize;
            self.raw_buffer.write(offset as _, &row).unwrap();
        }
    }

    fn xor_pixel(&mut self, x: i32, y: i32, color: Color) {
        if x < 0 || y < 0 || x >= self.width || y >= self.height {
            return;
        }

        let offset = (((y as u32) * self.width() + (x as u32)) * self.bytes_per_pixel()) as usize;

        let mut buffer = vec![0; self.bytes_per_pixel() as usize];
        self.raw_buffer.read(offset as _, &mut buffer).unwrap();

        let raw = T::xor_color(*bytemuck::from_bytes(&buffer[..size_of::<T::DataType>()]), color);
        self.raw_buffer.write(offset as _, bytemuck::bytes_of(&raw)).unwrap();
    }

    /// The row pair of `put_pixel`/`get_pixel`: one bulk guest write / read.
    fn put_row_raw(&mut self, x: i32, y: i32, data: &[u8]) -> bool {
        let bpp = self.bytes_per_pixel() as usize;
        if x < 0 || y < 0 || data.is_empty() || data.len() % bpp != 0 {
            return false;
        }
        let px = (data.len() / bpp) as u32;
        if (y as u32) >= self.height() || (x as u32).saturating_add(px) > self.width() {
            return false;
        }
        let offset = ((y as u32) * self.width() + (x as u32)) as usize * bpp;

        self.raw_buffer.write(offset as _, data).is_ok()
    }

    fn get_row_raw(&self, x: i32, y: i32, out: &mut [u8]) -> bool {
        let bpp = self.bytes_per_pixel() as usize;
        if x < 0 || y < 0 || out.is_empty() || out.len() % bpp != 0 {
            return false;
        }
        let px = (out.len() / bpp) as u32;
        if (y as u32) >= self.height() || (x as u32).saturating_add(px) > self.width() {
            return false;
        }
        let offset = ((y as u32) * self.width() + (x as u32)) as usize * bpp;

        self.raw_buffer.read(offset as _, out).is_ok()
    }
}
