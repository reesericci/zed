use crate::{
    px, AnyView, AppContext, AvailableSpace, Bounds, Context, Effect, Element, EntityId, FontId,
    GlyphId, GlyphRasterizationParams, Handle, Hsla, IsZero, LayoutId, MainThread, MainThreadOnly,
    MonochromeSprite, Pixels, PlatformAtlas, PlatformWindow, Point, Reference, Scene, Size,
    StackContext, StackingOrder, Style, TaffyLayoutEngine, WeakHandle, WindowOptions,
    SUBPIXEL_VARIANTS,
};
use anyhow::Result;
use futures::Future;
use smallvec::SmallVec;
use std::{any::TypeId, marker::PhantomData, mem, sync::Arc};
use util::ResultExt;

pub struct AnyWindow {}

pub struct Window {
    handle: AnyWindowHandle,
    platform_window: MainThreadOnly<Box<dyn PlatformWindow>>,
    glyph_atlas: Arc<dyn PlatformAtlas<GlyphRasterizationParams>>,
    rem_size: Pixels,
    content_size: Size<Pixels>,
    layout_engine: TaffyLayoutEngine,
    pub(crate) root_view: Option<AnyView<()>>,
    mouse_position: Point<Pixels>,
    current_layer_id: StackingOrder,
    pub(crate) scene: Scene,
    pub(crate) dirty: bool,
}

impl Window {
    pub fn new(
        handle: AnyWindowHandle,
        options: WindowOptions,
        cx: &mut MainThread<AppContext>,
    ) -> Self {
        let platform_window = cx.platform().open_window(handle, options);
        let glyph_atlas = platform_window.glyph_atlas();
        let mouse_position = platform_window.mouse_position();
        let content_size = platform_window.content_size();
        let scale_factor = platform_window.scale_factor();
        platform_window.on_resize(Box::new({
            let handle = handle;
            let cx = cx.to_async();
            move |content_size, scale_factor| {
                cx.update_window(handle, |cx| {
                    cx.window.scene = Scene::new(scale_factor);
                    cx.window.content_size = content_size;
                    cx.window.dirty = true;
                })
                .log_err();
            }
        }));

        let platform_window =
            MainThreadOnly::new(Arc::new(platform_window), cx.platform().dispatcher());

        Window {
            handle,
            platform_window,
            glyph_atlas,
            rem_size: px(16.),
            content_size,
            layout_engine: TaffyLayoutEngine::new(),
            root_view: None,
            mouse_position,
            current_layer_id: SmallVec::new(),
            scene: Scene::new(scale_factor),
            dirty: true,
        }
    }
}

pub struct WindowContext<'a, 'w> {
    app: Reference<'a, AppContext>,
    window: Reference<'w, Window>,
}

impl<'a, 'w> WindowContext<'a, 'w> {
    pub(crate) fn mutable(app: &'a mut AppContext, window: &'w mut Window) -> Self {
        Self {
            app: Reference::Mutable(app),
            window: Reference::Mutable(window),
        }
    }

    pub fn notify(&mut self) {
        self.window.dirty = true;
    }

    pub fn request_layout(
        &mut self,
        style: Style,
        children: impl IntoIterator<Item = LayoutId>,
    ) -> Result<LayoutId> {
        self.app.layout_id_buffer.clear();
        self.app.layout_id_buffer.extend(children.into_iter());
        let rem_size = self.rem_size();

        self.window
            .layout_engine
            .request_layout(style, rem_size, &self.app.layout_id_buffer)
    }

    pub fn request_measured_layout<
        F: Fn(Size<Option<Pixels>>, Size<AvailableSpace>) -> Size<Pixels> + Send + Sync + 'static,
    >(
        &mut self,
        style: Style,
        rem_size: Pixels,
        measure: F,
    ) -> Result<LayoutId> {
        self.window
            .layout_engine
            .request_measured_layout(style, rem_size, measure)
    }

    pub fn layout(&mut self, layout_id: LayoutId) -> Result<Layout> {
        Ok(self
            .window
            .layout_engine
            .layout(layout_id)
            .map(Into::into)?)
    }

    pub fn scale_factor(&self) -> f32 {
        self.window.scene.scale_factor
    }

    pub fn rem_size(&self) -> Pixels {
        self.window.rem_size
    }

    pub fn mouse_position(&self) -> Point<Pixels> {
        self.window.mouse_position
    }

    pub fn scene(&mut self) -> &mut Scene {
        &mut self.window.scene
    }

    pub fn stack<R>(&mut self, order: u32, f: impl FnOnce(&mut Self) -> R) -> R {
        self.window.current_layer_id.push(order);
        let result = f(self);
        self.window.current_layer_id.pop();
        result
    }

    pub fn current_layer_id(&self) -> StackingOrder {
        self.window.current_layer_id.clone()
    }

    pub fn run_on_main<R>(
        &self,
        f: impl FnOnce(&mut MainThread<WindowContext>) -> R + Send + 'static,
    ) -> impl Future<Output = Result<R>>
    where
        R: Send + 'static,
    {
        let id = self.window.handle.id;
        self.app.run_on_main(move |cx| {
            cx.update_window(id, |cx| {
                f(unsafe {
                    mem::transmute::<&mut WindowContext, &mut MainThread<WindowContext>>(cx)
                })
            })
        })
    }

    pub fn paint_glyph(
        &mut self,
        origin: Point<Pixels>,
        order: u32,
        font_id: FontId,
        glyph_id: GlyphId,
        font_size: Pixels,
        color: Hsla,
    ) -> Result<()> {
        let scale_factor = self.scale_factor();
        let glyph_origin = origin.scale(scale_factor);
        let subpixel_variant = Point {
            x: (glyph_origin.x.0.fract() * SUBPIXEL_VARIANTS as f32).floor() as u8,
            y: (glyph_origin.y.0.fract() * SUBPIXEL_VARIANTS as f32).floor() as u8,
        };
        let params = GlyphRasterizationParams {
            font_id,
            glyph_id,
            font_size,
            subpixel_variant,
            scale_factor,
        };

        let raster_bounds = self.text_system().raster_bounds(&params)?;

        if !raster_bounds.is_zero() {
            let layer_id = self.current_layer_id();
            let offset = raster_bounds.origin.map(Into::into);
            let glyph_origin = glyph_origin.map(|px| px.floor()) + offset;
            // dbg!(glyph_origin);
            let bounds = Bounds {
                origin: glyph_origin,
                size: raster_bounds.size.map(Into::into),
            };

            let tile = self
                .window
                .glyph_atlas
                .get_or_insert_with(&params, &mut || self.text_system().rasterize_glyph(&params))?;

            self.window.scene.insert(
                layer_id,
                MonochromeSprite {
                    order,
                    bounds,
                    clip_bounds: bounds,
                    clip_corner_radii: Default::default(),
                    color,
                    tile,
                },
            );
        }
        Ok(())
    }

    pub(crate) fn draw(&mut self) -> Result<()> {
        let unit_entity = self.unit_entity.clone();
        self.update_entity(&unit_entity, |_, cx| {
            let mut root_view = cx.window.root_view.take().unwrap();
            let (root_layout_id, mut frame_state) = root_view.layout(&mut (), cx)?;
            let available_space = cx.window.content_size.map(Into::into);

            cx.window
                .layout_engine
                .compute_layout(root_layout_id, available_space)?;
            let layout = cx.window.layout_engine.layout(root_layout_id)?;

            root_view.paint(layout, &mut (), &mut frame_state, cx)?;
            cx.window.root_view = Some(root_view);
            let scene = cx.window.scene.take();

            let _ = cx.run_on_main(|cx| {
                cx.window
                    .platform_window
                    .borrow_on_main_thread()
                    .draw(scene);
            });

            cx.window.dirty = false;
            Ok(())
        })
    }
}

impl Context for WindowContext<'_, '_> {
    type EntityContext<'a, 'w, T: 'static + Send + Sync> = ViewContext<'a, 'w, T>;
    type Result<T> = T;

    fn entity<T: Send + Sync + 'static>(
        &mut self,
        build_entity: impl FnOnce(&mut Self::EntityContext<'_, '_, T>) -> T,
    ) -> Handle<T> {
        let slot = self.app.entities.reserve();
        let entity = build_entity(&mut ViewContext::mutable(
            &mut *self.app,
            &mut self.window,
            slot.id,
        ));
        self.entities.redeem(slot, entity)
    }

    fn update_entity<T: Send + Sync + 'static, R>(
        &mut self,
        handle: &Handle<T>,
        update: impl FnOnce(&mut T, &mut Self::EntityContext<'_, '_, T>) -> R,
    ) -> R {
        let mut entity = self.entities.lease(handle);
        let result = update(
            &mut *entity,
            &mut ViewContext::mutable(&mut *self.app, &mut *self.window, handle.id),
        );
        self.entities.end_lease(entity);
        result
    }
}

impl<'a, 'w> std::ops::Deref for WindowContext<'a, 'w> {
    type Target = AppContext;

    fn deref(&self) -> &Self::Target {
        &self.app
    }
}

impl<'a, 'w> std::ops::DerefMut for WindowContext<'a, 'w> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.app
    }
}

impl<S> StackContext for ViewContext<'_, '_, S> {
    fn app(&mut self) -> &mut AppContext {
        &mut *self.window_cx.app
    }

    fn with_text_style<F, R>(&mut self, style: crate::TextStyleRefinement, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.window_cx.app.push_text_style(style);
        let result = f(self);
        self.window_cx.app.pop_text_style();
        result
    }

    fn with_state<T: Send + Sync + 'static, F, R>(&mut self, state: T, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.window_cx.app.push_state(state);
        let result = f(self);
        self.window_cx.app.pop_state::<T>();
        result
    }
}

pub struct ViewContext<'a, 'w, S> {
    window_cx: WindowContext<'a, 'w>,
    entity_type: PhantomData<S>,
    entity_id: EntityId,
}

impl<'a, 'w, S: Send + Sync + 'static> ViewContext<'a, 'w, S> {
    fn mutable(app: &'a mut AppContext, window: &'w mut Window, entity_id: EntityId) -> Self {
        Self {
            window_cx: WindowContext::mutable(app, window),
            entity_id,
            entity_type: PhantomData,
        }
    }

    pub fn handle(&self) -> WeakHandle<S> {
        self.entities.weak_handle(self.entity_id)
    }

    pub fn observe<E: Send + Sync + 'static>(
        &mut self,
        handle: &Handle<E>,
        on_notify: impl Fn(&mut S, Handle<E>, &mut ViewContext<'_, '_, S>) + Send + Sync + 'static,
    ) {
        let this = self.handle();
        let handle = handle.downgrade();
        let window_handle = self.window.handle;
        self.app
            .observers
            .entry(handle.id)
            .or_default()
            .push(Arc::new(move |cx| {
                cx.update_window(window_handle.id, |cx| {
                    if let Some(handle) = handle.upgrade(cx) {
                        this.update(cx, |this, cx| on_notify(this, handle, cx))
                            .is_ok()
                    } else {
                        false
                    }
                })
                .unwrap_or(false)
            }));
    }

    pub fn notify(&mut self) {
        self.window_cx.notify();
        self.window_cx
            .app
            .pending_effects
            .push_back(Effect::Notify(self.entity_id));
    }

    pub(crate) fn erase_state<R>(&mut self, f: impl FnOnce(&mut ViewContext<()>) -> R) -> R {
        let entity_id = self.unit_entity.id;
        let mut cx = ViewContext::mutable(
            &mut *self.window_cx.app,
            &mut *self.window_cx.window,
            entity_id,
        );
        f(&mut cx)
    }
}

impl<'a, 'w, S> Context for ViewContext<'a, 'w, S> {
    type EntityContext<'b, 'c, U: 'static + Send + Sync> = ViewContext<'b, 'c, U>;
    type Result<U> = U;

    fn entity<T2: Send + Sync + 'static>(
        &mut self,
        build_entity: impl FnOnce(&mut Self::EntityContext<'_, '_, T2>) -> T2,
    ) -> Handle<T2> {
        self.window_cx.entity(build_entity)
    }

    fn update_entity<U: Send + Sync + 'static, R>(
        &mut self,
        handle: &Handle<U>,
        update: impl FnOnce(&mut U, &mut Self::EntityContext<'_, '_, U>) -> R,
    ) -> R {
        self.window_cx.update_entity(handle, update)
    }
}

impl<'a, 'w, S: 'static> std::ops::Deref for ViewContext<'a, 'w, S> {
    type Target = WindowContext<'a, 'w>;

    fn deref(&self) -> &Self::Target {
        &self.window_cx
    }
}

impl<'a, 'w, S: 'static> std::ops::DerefMut for ViewContext<'a, 'w, S> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.window_cx
    }
}

// #[derive(Clone, Copy, Eq, PartialEq, Hash)]
slotmap::new_key_type! { pub struct WindowId; }

#[derive(PartialEq, Eq)]
pub struct WindowHandle<S> {
    id: WindowId,
    state_type: PhantomData<S>,
}

impl<S> Copy for WindowHandle<S> {}

impl<S> Clone for WindowHandle<S> {
    fn clone(&self) -> Self {
        WindowHandle {
            id: self.id,
            state_type: PhantomData,
        }
    }
}

impl<S> WindowHandle<S> {
    pub fn new(id: WindowId) -> Self {
        WindowHandle {
            id,
            state_type: PhantomData,
        }
    }
}

impl<S: 'static> Into<AnyWindowHandle> for WindowHandle<S> {
    fn into(self) -> AnyWindowHandle {
        AnyWindowHandle {
            id: self.id,
            state_type: TypeId::of::<S>(),
        }
    }
}

#[derive(Copy, Clone, PartialEq, Eq)]
pub struct AnyWindowHandle {
    pub(crate) id: WindowId,
    state_type: TypeId,
}

#[derive(Clone)]
pub struct Layout {
    pub order: u32,
    pub bounds: Bounds<Pixels>,
}
