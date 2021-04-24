use super::{
    super::{
        buffer::Buffer,
        consts::Consts,
        instances::Instances,
        model::{DynamicModel, Model, SubModel},
        pipelines::{
            blit, clouds, figure, fluid, lod_terrain, particle, postprocess, shadow, skybox,
            sprite, terrain, ui, ColLights, GlobalsBindGroup, Light, Shadow,
        },
    },
    Renderer, ShadowMap, ShadowMapRenderer,
};
use core::{num::NonZeroU32, ops::Range};
use std::sync::Arc;
use vek::Aabr;
use wgpu_profiler::scope::{ManualOwningScope, OwningScope, Scope};

// Borrow the fields we need from the renderer so that the GpuProfiler can be
// dijointly borrowed mutably
struct RendererBorrow<'frame> {
    queue: &'frame wgpu::Queue,
    device: &'frame wgpu::Device,
    shadow: &'frame super::Shadow,
    pipelines: &'frame super::Pipelines,
    locals: &'frame super::locals::Locals,
    views: &'frame super::Views,
    mode: &'frame super::super::RenderMode,
    quad_index_buffer_u16: &'frame Buffer<u16>,
    quad_index_buffer_u32: &'frame Buffer<u32>,
}

pub struct Drawer<'frame> {
    encoder: Option<ManualOwningScope<'frame, wgpu::CommandEncoder>>,
    borrow: RendererBorrow<'frame>,
    swap_tex: wgpu::SwapChainTexture,
    globals: &'frame GlobalsBindGroup,
    // Texture and other info for taking a screenshot
    // Writes to this instead in the third pass if it is present
    taking_screenshot: Option<super::screenshot::TakeScreenshot>,
}

impl<'frame> Drawer<'frame> {
    pub fn new(
        encoder: wgpu::CommandEncoder,
        renderer: &'frame mut Renderer,
        swap_tex: wgpu::SwapChainTexture,
        globals: &'frame GlobalsBindGroup,
    ) -> Self {
        let taking_screenshot = renderer.take_screenshot.take().map(|screenshot_fn| {
            super::screenshot::TakeScreenshot::new(
                &renderer.device,
                &renderer.layouts.blit,
                &renderer.sampler,
                &renderer.sc_desc,
                screenshot_fn,
            )
        });

        let borrow = RendererBorrow {
            queue: &renderer.queue,
            device: &renderer.device,
            shadow: &renderer.shadow,
            pipelines: &renderer.pipelines,
            locals: &renderer.locals,
            views: &renderer.views,
            mode: &renderer.mode,
            quad_index_buffer_u16: &renderer.quad_index_buffer_u16,
            quad_index_buffer_u32: &renderer.quad_index_buffer_u32,
        };

        let mut encoder =
            ManualOwningScope::start("frame", &mut renderer.profiler, encoder, borrow.device);

        Self {
            encoder: Some(encoder),
            borrow,
            swap_tex,
            globals,
            taking_screenshot,
        }
    }

    /// Get the render mode.
    pub fn render_mode(&self) -> &super::super::RenderMode { self.borrow.mode }

    pub fn shadow_pass(&mut self) -> Option<ShadowPassDrawer> {
        if let ShadowMap::Enabled(ref shadow_renderer) = self.borrow.shadow.map {
            let encoder = self.encoder.as_mut().unwrap();
            let device = self.borrow.device;
            let mut render_pass =
                encoder.scoped_render_pass("shadow_pass", device, &wgpu::RenderPassDescriptor {
                    label: Some("shadow pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(
                        wgpu::RenderPassDepthStencilAttachment {
                            view: &shadow_renderer.directed_depth.view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        },
                    ),
                });

            render_pass.set_bind_group(0, &self.globals.bind_group, &[]);

            Some(ShadowPassDrawer {
                render_pass,
                borrow: &self.borrow,
                shadow_renderer,
            })
        } else {
            None
        }
    }

    pub fn first_pass(&mut self) -> FirstPassDrawer {
        let encoder = self.encoder.as_mut().unwrap();
        let device = self.borrow.device;
        let mut render_pass =
            encoder.scoped_render_pass("first_pass", device, &wgpu::RenderPassDescriptor {
                label: Some("first pass"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &self.borrow.views.tgt_color,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: true,
                    },
                }],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.borrow.views.tgt_depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0),
                        store: true,
                    }),
                    stencil_ops: None,
                }),
            });

        render_pass.set_bind_group(0, &self.globals.bind_group, &[]);
        render_pass.set_bind_group(1, &self.borrow.shadow.bind.bind_group, &[]);

        FirstPassDrawer {
            render_pass,
            borrow: &self.borrow,
            globals: self.globals,
        }
    }

    pub fn second_pass(&mut self) -> SecondPassDrawer {
        let encoder = self.encoder.as_mut().unwrap();
        let device = self.borrow.device;
        let mut render_pass =
            encoder.scoped_render_pass("second_pass", device, &wgpu::RenderPassDescriptor {
                label: Some("second pass (clouds)"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    view: &self.borrow.views.tgt_color_pp,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

        render_pass.set_bind_group(0, &self.globals.bind_group, &[]);
        render_pass.set_bind_group(1, &self.borrow.shadow.bind.bind_group, &[]);

        SecondPassDrawer {
            render_pass,
            borrow: &self.borrow,
        }
    }

    pub fn third_pass(&mut self) -> ThirdPassDrawer {
        let encoder = self.encoder.as_mut().unwrap();
        let device = self.borrow.device;
        let mut render_pass =
            encoder.scoped_render_pass("third_pass", device, &wgpu::RenderPassDescriptor {
                label: Some("third pass (postprocess + ui)"),
                color_attachments: &[wgpu::RenderPassColorAttachment {
                    // If a screenshot was requested render to that as an intermediate texture
                    // instead
                    view: self
                        .taking_screenshot
                        .as_ref()
                        .map_or(&self.swap_tex.view, |s| s.texture_view()),
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: true,
                    },
                }],
                depth_stencil_attachment: None,
            });

        render_pass.set_bind_group(0, &self.globals.bind_group, &[]);

        ThirdPassDrawer {
            render_pass,
            borrow: &self.borrow,
        }
    }

    pub fn draw_point_shadows<'data: 'frame>(
        &mut self,
        matrices: &[shadow::PointLightMatrix; 126],
        chunks: impl Clone
        + Iterator<Item = (&'data Model<terrain::Vertex>, &'data terrain::BoundLocals)>,
    ) {
        if let ShadowMap::Enabled(ref shadow_renderer) = self.borrow.shadow.map {
            let device = self.borrow.device;
            let mut encoder = self
                .encoder
                .as_mut()
                .unwrap()
                .scope("point shadows", device);
            const STRIDE: usize = std::mem::size_of::<shadow::PointLightMatrix>();
            let data = bytemuck::cast_slice(matrices);

            for face in 0..6 {
                // TODO: view creation cost?
                let view =
                    shadow_renderer
                        .point_depth
                        .tex
                        .create_view(&wgpu::TextureViewDescriptor {
                            label: Some("Point shadow cubemap face"),
                            format: None,
                            dimension: Some(wgpu::TextureViewDimension::D2),
                            aspect: wgpu::TextureAspect::DepthOnly,
                            base_mip_level: 0,
                            mip_level_count: None,
                            base_array_layer: face,
                            array_layer_count: NonZeroU32::new(1),
                        });

                let label = format!("point shadow face-{} pass", face);
                let mut render_pass =
                    encoder.scoped_render_pass(&label, device, &wgpu::RenderPassDescriptor {
                        label: Some(&label),
                        color_attachments: &[],
                        depth_stencil_attachment: Some(
                            wgpu::RenderPassDepthStencilAttachment {
                                view: &view,
                                depth_ops: Some(wgpu::Operations {
                                    load: wgpu::LoadOp::Clear(1.0),
                                    store: true,
                                }),
                                stencil_ops: None,
                            },
                        ),
                    });

                render_pass.set_pipeline(&shadow_renderer.point_pipeline.pipeline);
                set_quad_index_buffer::<terrain::Vertex>(&mut render_pass, &self.borrow);
                render_pass.set_bind_group(0, &self.globals.bind_group, &[]);

                (0../*20*/1).for_each(|point_light| {
                    render_pass.set_push_constants(
                        wgpu::ShaderStage::all(),
                        0,
                        &data[(6 * (point_light + 1) * STRIDE + face as usize * STRIDE)
                            ..(6 * (point_light + 1) * STRIDE + (face + 1) as usize * STRIDE)],
                    );
                    chunks.clone().for_each(|(model, locals)| {
                        render_pass.set_bind_group(1, &locals.bind_group, &[]);
                        render_pass.set_vertex_buffer(0, model.buf().slice(..));
                        render_pass.draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..1);
                    });
                });
            }
        }
    }

    /// Clear all the shadow textures, useful if directed shadows (shadow_pass)
    /// and point light shadows (draw_point_shadows) are unused and thus the
    /// textures will otherwise not be cleared after either their
    /// initialization or their last use
    /// NOTE: could simply use the above passes except `draw_point_shadows`
    /// requires an array of matrices that could be a pain to construct
    /// simply for clearing
    pub fn clear_shadows(&mut self) {
        if let ShadowMap::Enabled(ref shadow_renderer) = self.borrow.shadow.map {
            let device = self.borrow.device;
            let encoder = self.encoder.as_mut().unwrap();
            let _ = encoder.scoped_render_pass(
                "clear_directed_shadow",
                device,
                &wgpu::RenderPassDescriptor {
                    label: Some("clear directed shadow pass"),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(
                        wgpu::RenderPassDepthStencilAttachment {
                            view: &shadow_renderer.directed_depth.view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        },
                    ),
                },
            );

            for face in 0..6 {
                // TODO: view creation cost?
                let view =
                    shadow_renderer
                        .point_depth
                        .tex
                        .create_view(&wgpu::TextureViewDescriptor {
                            label: Some("Point shadow cubemap face"),
                            format: None,
                            dimension: Some(wgpu::TextureViewDimension::D2),
                            aspect: wgpu::TextureAspect::DepthOnly,
                            base_mip_level: 0,
                            mip_level_count: None,
                            base_array_layer: face,
                            array_layer_count: NonZeroU32::new(1),
                        });

                let label = format!("clear point shadow face-{} pass", face);
                let _ = encoder.scoped_render_pass(&label, device, &wgpu::RenderPassDescriptor {
                    label: Some(&label),
                    color_attachments: &[],
                    depth_stencil_attachment: Some(
                        wgpu::RenderPassDepthStencilAttachment {
                            view: &view,
                            depth_ops: Some(wgpu::Operations {
                                load: wgpu::LoadOp::Clear(1.0),
                                store: true,
                            }),
                            stencil_ops: None,
                        },
                    ),
                });
            }
        }
    }
}

impl<'frame> Drop for Drawer<'frame> {
    fn drop(&mut self) {
        // TODO: submitting things to the queue can let the gpu start on them sooner
        // maybe we should submit each render pass to the queue as they are produced?
        let mut encoder = self.encoder.take().unwrap();

        // If taking a screenshot
        if let Some(screenshot) = self.taking_screenshot.take() {
            // Image needs to be copied from the screenshot texture to the swapchain texture
            let mut render_pass = encoder.scoped_render_pass(
                "screenshot blit",
                self.borrow.device,
                &wgpu::RenderPassDescriptor {
                    label: Some("Blit screenshot pass"),
                    color_attachments: &[wgpu::RenderPassColorAttachment {
                        view: &self.swap_tex.view,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: true,
                        },
                    }],
                    depth_stencil_attachment: None,
                },
            );
            render_pass.set_pipeline(&self.borrow.pipelines.blit.pipeline);
            render_pass.set_bind_group(0, &screenshot.bind_group(), &[]);
            render_pass.draw(0..3, 0..1);
            drop(render_pass);
            // Issues a command to copy from the texture to a buffer and then sends the
            // buffer off to another thread to be mapped and processed
            screenshot.download_and_handle(&mut encoder);
        }

        let (mut encoder, profiler) = encoder.end_scope();
        profiler.resolve_queries(&mut encoder);

        self.borrow.queue.submit(std::iter::once(encoder.finish()));

        profiler
            .end_frame()
            .expect("Gpu profiler error! Maybe there was an unclosed scope?");
    }
}

// Shadow pass
pub struct ShadowPassDrawer<'pass> {
    render_pass: OwningScope<'pass, wgpu::RenderPass<'pass>>,
    borrow: &'pass RendererBorrow<'pass>,
    shadow_renderer: &'pass ShadowMapRenderer,
}

impl<'pass> ShadowPassDrawer<'pass> {
    pub fn draw_figure_shadows(&mut self) -> FigureShadowDrawer<'_, 'pass> {
        let mut render_pass = self
            .render_pass
            .scope("direcred_figure_shadows", self.borrow.device);

        render_pass.set_pipeline(&self.shadow_renderer.figure_directed_pipeline.pipeline);
        set_quad_index_buffer::<terrain::Vertex>(&mut render_pass, &self.borrow);

        FigureShadowDrawer { render_pass }
    }

    pub fn draw_terrain_shadows(&mut self) -> TerrainShadowDrawer<'_, 'pass> {
        let mut render_pass = self
            .render_pass
            .scope("direcred_terrain_shadows", self.borrow.device);

        render_pass.set_pipeline(&self.shadow_renderer.terrain_directed_pipeline.pipeline);
        set_quad_index_buffer::<terrain::Vertex>(&mut render_pass, &self.borrow);

        TerrainShadowDrawer { render_pass }
    }
}

pub struct FigureShadowDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
}

impl<'pass_ref, 'pass: 'pass_ref> FigureShadowDrawer<'pass_ref, 'pass> {
    pub fn draw<'data: 'pass>(
        &mut self,
        model: SubModel<'data, terrain::Vertex>,
        locals: &'data figure::BoundLocals,
    ) {
        self.render_pass.set_bind_group(1, &locals.bind_group, &[]);
        self.render_pass.set_vertex_buffer(0, model.buf());
        self.render_pass
            .draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..1);
    }
}

pub struct TerrainShadowDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
}

impl<'pass_ref, 'pass: 'pass_ref> TerrainShadowDrawer<'pass_ref, 'pass> {
    pub fn draw<'data: 'pass>(
        &mut self,
        model: &'data Model<terrain::Vertex>,
        locals: &'data terrain::BoundLocals,
    ) {
        self.render_pass.set_bind_group(1, &locals.bind_group, &[]);
        self.render_pass.set_vertex_buffer(0, model.buf().slice(..));
        self.render_pass
            .draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..1);
    }
}

// First pass
pub struct FirstPassDrawer<'pass> {
    pub(super) render_pass: OwningScope<'pass, wgpu::RenderPass<'pass>>,
    borrow: &'pass RendererBorrow<'pass>,
    globals: &'pass GlobalsBindGroup,
}

impl<'pass> FirstPassDrawer<'pass> {
    pub fn draw_skybox<'data: 'pass>(&mut self, model: &'data Model<skybox::Vertex>) {
        let mut render_pass = self.render_pass.scope("skybox", self.borrow.device);

        render_pass.set_pipeline(&self.borrow.pipelines.skybox.pipeline);
        set_quad_index_buffer::<skybox::Vertex>(&mut render_pass, &self.borrow);
        render_pass.set_vertex_buffer(0, model.buf().slice(..));
        render_pass.draw(0..model.len() as u32, 0..1);
    }

    pub fn draw_lod_terrain<'data: 'pass>(&mut self, model: &'data Model<lod_terrain::Vertex>) {
        let mut render_pass = self.render_pass.scope("lod_terrain", self.borrow.device);

        render_pass.set_pipeline(&self.borrow.pipelines.lod_terrain.pipeline);
        set_quad_index_buffer::<lod_terrain::Vertex>(&mut render_pass, &self.borrow);
        render_pass.set_vertex_buffer(0, model.buf().slice(..));
        render_pass.draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..1);
    }

    pub fn draw_figures(&mut self) -> FigureDrawer<'_, 'pass> {
        let mut render_pass = self.render_pass.scope("figures", self.borrow.device);

        render_pass.set_pipeline(&self.borrow.pipelines.figure.pipeline);
        set_quad_index_buffer::<terrain::Vertex>(&mut render_pass, &self.borrow);

        FigureDrawer { render_pass }
    }

    pub fn draw_terrain<'data: 'pass>(&mut self) -> TerrainDrawer<'_, 'pass> {
        let mut render_pass = self.render_pass.scope("terrain", self.borrow.device);

        render_pass.set_pipeline(&self.borrow.pipelines.terrain.pipeline);
        set_quad_index_buffer::<terrain::Vertex>(&mut render_pass, &self.borrow);

        TerrainDrawer {
            render_pass,
            col_lights: None,
        }
    }

    pub fn draw_particles(&mut self) -> ParticleDrawer<'_, 'pass> {
        let mut render_pass = self.render_pass.scope("particles", self.borrow.device);

        render_pass.set_pipeline(&self.borrow.pipelines.particle.pipeline);
        set_quad_index_buffer::<particle::Vertex>(&mut render_pass, &self.borrow);

        ParticleDrawer { render_pass }
    }

    pub fn draw_sprites<'data: 'pass>(
        &mut self,
        globals: &'data sprite::SpriteGlobalsBindGroup,
        col_lights: &'data ColLights<sprite::Locals>,
    ) -> SpriteDrawer<'_, 'pass> {
        let mut render_pass = self.render_pass.scope("sprites", self.borrow.device);

        render_pass.set_pipeline(&self.borrow.pipelines.sprite.pipeline);
        set_quad_index_buffer::<particle::Vertex>(&mut render_pass, &self.borrow);
        render_pass.set_bind_group(0, &globals.bind_group, &[]);
        render_pass.set_bind_group(3, &col_lights.bind_group, &[]);

        SpriteDrawer {
            render_pass,
            globals: self.globals,
        }
    }

    pub fn draw_fluid<'data: 'pass>(
        &mut self,
        waves: &'data fluid::BindGroup,
    ) -> FluidDrawer<'_, 'pass> {
        let mut render_pass = self.render_pass.scope("fluid", self.borrow.device);

        render_pass.set_pipeline(&self.borrow.pipelines.fluid.pipeline);
        set_quad_index_buffer::<fluid::Vertex>(&mut render_pass, &self.borrow);
        render_pass.set_bind_group(2, &waves.bind_group, &[]);

        FluidDrawer { render_pass }
    }
}

pub struct FigureDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
}

impl<'pass_ref, 'pass: 'pass_ref> FigureDrawer<'pass_ref, 'pass> {
    pub fn draw<'data: 'pass>(
        &mut self,
        model: SubModel<'data, terrain::Vertex>,
        locals: &'data figure::BoundLocals,
        // TODO: don't rebind this every time once they are shared between figures
        col_lights: &'data ColLights<figure::Locals>,
    ) {
        self.render_pass.set_bind_group(2, &locals.bind_group, &[]);
        self.render_pass
            .set_bind_group(3, &col_lights.bind_group, &[]);
        self.render_pass.set_vertex_buffer(0, model.buf());
        self.render_pass
            .draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..1);
    }
}

pub struct TerrainDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
    col_lights: Option<&'pass_ref Arc<ColLights<terrain::Locals>>>,
}

impl<'pass_ref, 'pass: 'pass_ref> TerrainDrawer<'pass_ref, 'pass> {
    pub fn draw<'data: 'pass>(
        &mut self,
        model: &'data Model<terrain::Vertex>,
        col_lights: &'data Arc<ColLights<terrain::Locals>>,
        locals: &'data terrain::BoundLocals,
    ) {
        let col_lights = if let Some(col_lights) = self
            .col_lights
            // Check if we are still using the same atlas texture as the previous drawn
            // chunk
            .filter(|current_col_lights| Arc::ptr_eq(current_col_lights, col_lights))
        {
            col_lights
        } else {
            self.render_pass
                .set_bind_group(3, &col_lights.bind_group, &[]); // TODO: put this in slot 2
            self.col_lights = Some(col_lights);
            col_lights
        };

        self.render_pass.set_bind_group(2, &locals.bind_group, &[]); // TODO: put this in slot 3
        self.render_pass.set_vertex_buffer(0, model.buf().slice(..));
        self.render_pass
            .draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..1);
    }
}

pub struct ParticleDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
}

impl<'pass_ref, 'pass: 'pass_ref> ParticleDrawer<'pass_ref, 'pass> {
    // Note: if we ever need to draw less than the whole model, these APIs can be
    // changed
    pub fn draw<'data: 'pass>(
        &mut self,
        model: &'data Model<particle::Vertex>,
        instances: &'data Instances<particle::Instance>,
    ) {
        self.render_pass.set_vertex_buffer(0, model.buf().slice(..));
        self.render_pass
            .set_vertex_buffer(1, instances.buf().slice(..));
        self.render_pass
            // TODO: since we cast to u32 maybe this should returned by the len/count functions?
            .draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..instances.count() as u32);
    }
}

pub struct SpriteDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
    globals: &'pass GlobalsBindGroup,
}

impl<'pass_ref, 'pass: 'pass_ref> SpriteDrawer<'pass_ref, 'pass> {
    pub fn draw<'data: 'pass>(
        &mut self,
        terrain_locals: &'data terrain::BoundLocals,
        //model: &'data Model<sprite::Vertex>,
        //locals: &'data sprite::BoundLocals,
        instances: &'data Instances<sprite::Instance>,
    ) {
        self.render_pass
            .set_bind_group(2, &terrain_locals.bind_group, &[]);
        //self.render_pass.set_bind_group(3, &locals.bind_group, &[]);

        //self.render_pass.set_vertex_buffer(0, model.buf().slice(..));
        self.render_pass
            .set_vertex_buffer(0, instances.buf().slice(..));
        self.render_pass.draw_indexed(
            0..sprite::VERT_PAGE_SIZE / 4 * 6,
            0,
            0..instances.count() as u32,
        );
    }
}

impl<'pass_ref, 'pass: 'pass_ref> Drop for SpriteDrawer<'pass_ref, 'pass> {
    fn drop(&mut self) {
        // Reset to regular globals
        self.render_pass
            .set_bind_group(0, &self.globals.bind_group, &[]);
    }
}

pub struct FluidDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
}

impl<'pass_ref, 'pass: 'pass_ref> FluidDrawer<'pass_ref, 'pass> {
    pub fn draw<'data: 'pass>(
        &mut self,
        model: &'data Model<fluid::Vertex>,
        locals: &'data terrain::BoundLocals,
    ) {
        self.render_pass.set_vertex_buffer(0, model.buf().slice(..));
        self.render_pass.set_bind_group(3, &locals.bind_group, &[]);
        self.render_pass
            .draw_indexed(0..model.len() as u32 / 4 * 6, 0, 0..1);
    }
}

// Second pass: clouds
pub struct SecondPassDrawer<'pass> {
    render_pass: OwningScope<'pass, wgpu::RenderPass<'pass>>,
    borrow: &'pass RendererBorrow<'pass>,
}

impl<'pass> SecondPassDrawer<'pass> {
    pub fn draw_clouds(&mut self) {
        self.render_pass
            .set_pipeline(&self.borrow.pipelines.clouds.pipeline);
        self.render_pass
            .set_bind_group(2, &self.borrow.locals.clouds_bind.bind_group, &[]);
        self.render_pass.draw(0..3, 0..1);
    }
}

// Third pass: postprocess + ui
pub struct ThirdPassDrawer<'pass> {
    render_pass: OwningScope<'pass, wgpu::RenderPass<'pass>>,
    borrow: &'pass RendererBorrow<'pass>,
}

impl<'pass> ThirdPassDrawer<'pass> {
    pub fn draw_post_process(&mut self) {
        let mut render_pass = self.render_pass.scope("postprocess", self.borrow.device);
        render_pass.set_pipeline(&self.borrow.pipelines.postprocess.pipeline);
        render_pass.set_bind_group(1, &self.borrow.locals.postprocess_bind.bind_group, &[]);
        render_pass.draw(0..3, 0..1);
    }

    pub fn draw_ui(&mut self) -> UiDrawer<'_, 'pass> {
        let mut render_pass = self.render_pass.scope("ui", self.borrow.device);
        render_pass.set_pipeline(&self.borrow.pipelines.ui.pipeline);
        set_quad_index_buffer::<ui::Vertex>(&mut render_pass, &self.borrow);

        UiDrawer { render_pass }
    }
}

pub struct UiDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: Scope<'pass_ref, wgpu::RenderPass<'pass>>,
}

pub struct PreparedUiDrawer<'pass_ref, 'pass: 'pass_ref> {
    render_pass: &'pass_ref mut wgpu::RenderPass<'pass>,
}

impl<'pass_ref, 'pass: 'pass_ref> UiDrawer<'pass_ref, 'pass> {
    /// Set vertex buffer, initial scissor, and locals
    /// These can be changed later but this ensures that they don't have to be
    /// set with every draw call
    pub fn prepare<'data: 'pass>(
        &mut self,
        locals: &'data ui::BoundLocals,
        //texture: &'data ui::TextureBindGroup,
        buf: &'data DynamicModel<ui::Vertex>,
        scissor: Aabr<u16>,
    ) -> PreparedUiDrawer<'_, 'pass> {
        // Note: not actually prepared yet
        // we do this to avoid having to write extra code for the set functions
        let mut prepared = PreparedUiDrawer {
            render_pass: &mut *self.render_pass,
        };
        // Prepare
        prepared.set_locals(locals);
        //prepared.set_texture(texture);
        prepared.set_model(buf);
        prepared.set_scissor(scissor);

        prepared
    }
}

impl<'pass_ref, 'pass: 'pass_ref> PreparedUiDrawer<'pass_ref, 'pass> {
    pub fn set_locals<'data: 'pass>(&mut self, locals: &'data ui::BoundLocals) {
        self.render_pass.set_bind_group(1, &locals.bind_group, &[]);
    }

    //pub fn set_texture<'data: 'pass>(&mut self, texture: &'data
    // ui::TextureBindGroup) {    self.render_pass.set_bind_group(1,
    // &texture.bind_group, &[]);
    //}

    pub fn set_model<'data: 'pass>(&mut self, model: &'data DynamicModel<ui::Vertex>) {
        self.render_pass.set_vertex_buffer(0, model.buf().slice(..))
    }

    pub fn set_scissor<'data: 'pass>(&mut self, scissor: Aabr<u16>) {
        let Aabr { min, max } = scissor;
        // TODO: Got an invalid scissor panic from wgpu,
        // use this if you can reproduce
        // Note: might have been from changing monitors
        // dbg!(&scissor)
        self.render_pass.set_scissor_rect(
            min.x as u32,
            min.y as u32,
            (max.x - min.x) as u32,
            (max.y - min.y) as u32,
        );
    }

    pub fn draw<'data: 'pass>(&mut self, texture: &'data ui::TextureBindGroup, verts: Range<u32>) {
        self.render_pass.set_bind_group(2, &texture.bind_group, &[]);
        self.render_pass.draw(verts, 0..1);
    }
}

fn set_quad_index_buffer<'a, V: super::super::Vertex>(
    pass: &mut wgpu::RenderPass<'a>,
    borrow: &RendererBorrow<'a>,
) {
    match V::QUADS_INDEX {
        Some(format) => {
            let slice = match format {
                wgpu::IndexFormat::Uint16 => borrow.quad_index_buffer_u16.buf.slice(..),
                wgpu::IndexFormat::Uint32 => borrow.quad_index_buffer_u32.buf.slice(..),
            };

            pass.set_index_buffer(slice, format);
        },
        None => {},
    }
}