//! Displays debug lines using an orthographic camera.

use amethyst::{
    core::{
        transform::{Transform, TransformBundle},
        Time,
    },
    ecs::{Read, ReadExpect, Resources, System, SystemData, Write},
    input::{is_close_requested, is_key_down},
    prelude::*,
    renderer::{
        camera::{Camera, Projection},
        debug_drawing::{DebugLines, DebugLinesComponent, DebugLinesParams},
        palette::Srgba,
        pass::DrawDebugLinesDesc,
        rendy::{
            factory::Factory,
            graph::{
                present::PresentNode,
                render::{RenderGroupDesc, SubpassBuilder},
                GraphBuilder,
            },
            hal::{
                command::{ClearDepthStencil, ClearValue},
                format::Format,
                image,
            },
        },
        types::DefaultBackend,
        Backend, GraphCreator, RenderingSystem,
    },
    utils::application_root_dir,
    window::{ScreenDimensions, Window, WindowBundle},
    winit::VirtualKeyCode,
};

struct ExampleLinesSystem;
impl<'s> System<'s> for ExampleLinesSystem {
    type SystemData = (
        ReadExpect<'s, ScreenDimensions>,
        Write<'s, DebugLines>,
        Read<'s, Time>,
    );

    fn run(&mut self, (screen_dimensions, mut debug_lines_resource, time): Self::SystemData) {
        let t = (time.absolute_time_seconds() as f32).cos() / 2.0 + 0.5;

        let screen_w = screen_dimensions.width();
        let screen_h = screen_dimensions.height();
        let y = t * screen_h;

        debug_lines_resource.draw_line(
            [0.0, y, 1.0].into(),
            [screen_w, y + 2.0, 1.0].into(),
            Srgba::new(0.3, 0.3, 1.0, 1.0),
        );
    }
}

struct ExampleState;

impl SimpleState for ExampleState {
    fn on_start(&mut self, data: StateData<'_, GameData<'_, '_>>) {
        // Setup debug lines as a resource
        data.world.add_resource(DebugLines::new());
        // Configure width of lines. Optional step
        data.world
            .add_resource(DebugLinesParams { line_width: 2.0 });

        // Setup debug lines as a component and add lines to render axis&grid
        let mut debug_lines_component = DebugLinesComponent::with_capacity(100);

        let (screen_w, screen_h) = {
            let screen_dimensions = data.world.read_resource::<ScreenDimensions>();
            (screen_dimensions.width(), screen_dimensions.height())
        };

        (0..(screen_h as u16))
            .step_by(50)
            .map(f32::from)
            .for_each(|y| {
                debug_lines_component.add_line(
                    [0.0, y, 1.0].into(),
                    [screen_w, (y + 2.0), 1.0].into(),
                    Srgba::new(0.3, 0.3, 0.3, 1.0),
                );
            });

        (0..(screen_w as u16))
            .step_by(50)
            .map(f32::from)
            .for_each(|x| {
                debug_lines_component.add_line(
                    [x, 0.0, 1.0].into(),
                    [x, screen_h, 1.0].into(),
                    Srgba::new(0.3, 0.3, 0.3, 1.0),
                );
            });

        debug_lines_component.add_line(
            [20.0, 20.0, 1.0].into(),
            [780.0, 580.0, 1.0].into(),
            Srgba::new(1.0, 0.0, 0.2, 1.0), // Red
        );

        data.world.register::<DebugLinesComponent>();
        data.world
            .create_entity()
            .with(debug_lines_component)
            .build();

        // Setup camera
        let mut local_transform = Transform::default();
        local_transform.set_translation_xyz(0.0, screen_h, 10.0);
        let left = 0.0;
        let right = screen_w;
        let bottom = 0.0;
        let top = screen_h;
        let znear = 0.0;
        let zfar = 100.0;
        data.world
            .create_entity()
            .with(Camera::from(Projection::orthographic(
                left, right, bottom, top, znear, zfar,
            )))
            .with(local_transform)
            .build();
    }

    fn handle_event(
        &mut self,
        _: StateData<'_, GameData<'_, '_>>,
        event: StateEvent,
    ) -> SimpleTrans {
        if let StateEvent::Window(event) = event {
            if is_close_requested(&event) || is_key_down(&event, VirtualKeyCode::Escape) {
                Trans::Quit
            } else {
                Trans::None
            }
        } else {
            Trans::None
        }
    }
}

fn main() -> amethyst::Result<()> {
    amethyst::start_logger(Default::default());

    let app_root = application_root_dir()?;

    let display_config_path = app_root.join("examples/debug_lines_ortho/resources/display.ron");
    let resources = app_root.join("examples/assets/");

    let game_data = GameDataBuilder::default()
        .with_bundle(WindowBundle::from_config_path(display_config_path))?
        .with_bundle(TransformBundle::new())?
        .with(ExampleLinesSystem, "example_lines_system", &[])
        .with_thread_local(RenderingSystem::<DefaultBackend, _>::new(
            ExampleGraph::default(),
        ));

    let mut game = Application::new(resources, ExampleState, game_data)?;
    game.run();
    Ok(())
}

#[derive(Default)]
struct ExampleGraph {
    dimensions: Option<ScreenDimensions>,
    surface_format: Option<Format>,
    dirty: bool,
}

impl<B: Backend> GraphCreator<B> for ExampleGraph {
    fn rebuild(&mut self, res: &Resources) -> bool {
        // Rebuild when dimensions change, but wait until at least two frames have the same.
        let new_dimensions = res.try_fetch::<ScreenDimensions>();
        use std::ops::Deref;
        if self.dimensions.as_ref() != new_dimensions.as_ref().map(|d| d.deref()) {
            self.dirty = true;
            self.dimensions = new_dimensions.map(|d| d.clone());
            return false;
        }
        return self.dirty;
    }

    fn builder(&mut self, factory: &mut Factory<B>, res: &Resources) -> GraphBuilder<B, Resources> {
        self.dirty = false;

        let window = <ReadExpect<'_, Window>>::fetch(res);

        let surface = factory.create_surface(&window);
        // cache surface format to speed things up
        let surface_format = *self
            .surface_format
            .get_or_insert_with(|| factory.get_surface_format(&surface));
        let dimensions = self.dimensions.as_ref().unwrap();
        let window_kind =
            image::Kind::D2(dimensions.width() as u32, dimensions.height() as u32, 1, 1);

        let mut graph_builder = GraphBuilder::new();
        let color = graph_builder.create_image(
            window_kind,
            1,
            surface_format,
            Some(ClearValue::Color([0.0, 0.0, 0.0, 1.0].into())),
        );

        let depth = graph_builder.create_image(
            window_kind,
            1,
            Format::D32Sfloat,
            Some(ClearValue::DepthStencil(ClearDepthStencil(1.0, 0))),
        );

        let opaque = graph_builder.add_node(
            SubpassBuilder::new()
                .with_group(DrawDebugLinesDesc::new().builder())
                .with_color(color)
                .with_depth_stencil(depth)
                .into_pass(),
        );

        let _present = graph_builder
            .add_node(PresentNode::builder(factory, surface, color).with_dependency(opaque));

        graph_builder
    }
}
