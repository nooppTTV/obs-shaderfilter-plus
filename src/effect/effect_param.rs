use std::sync::Arc;
use std::borrow::Cow;
use obs_wrapper::{obs_sys::MAX_AUDIO_MIXES, context::*, graphics::*, source::*};
use smallvec::{SmallVec, smallvec};
use paste::item;
use crate::*;

/// Used to convert cloneable values into `ShaderParamType::RustType`.
pub trait EffectParamType {
    type ShaderParamType: ShaderParamType;
    type PreparedValueType: Default;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &GraphicsContext,
    ) -> GraphicsContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType>;

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
        context: &'b FilterContext,
    );
}

macro_rules! define_effect_param_aliases {
    ($($name:ident),*$(,)?) => {
        item! {
            $(
                #[allow(dead_code)]
                pub type [< EffectParam $name >] = EffectParam<EffectParamTypeClone<[< ShaderParamType $name >]>>;
            )*
        }
    }
}

define_effect_param_aliases! {
    Bool, Int, IVec2, IVec3, IVec4, Float, Vec2, Vec3, Vec4, Mat4,
}

#[derive(Clone, Debug)]
pub struct TextureDescriptor {
    pub dimensions: [usize; 2],
    pub color_format: ColorFormatKind,
    pub levels: SmallVec<[Vec<u8>; 1]>,
    pub flags: u32,
}

impl Default for TextureDescriptor {
    fn default() -> Self {
        Self {
            dimensions: [1, 1],
            color_format: ColorFormatKind::RGBA,
            levels: smallvec![vec![0, 0, 0, 0]],
            flags: 0,
        }
    }
}

pub type EffectParamTexture = EffectParam<EffectParamTypeTexture>;

pub struct EffectParamTypeClone<T>
    where T: ShaderParamType,
          <T as ShaderParamType>::RustType: Default,
{
    __marker: std::marker::PhantomData<T>,
}

impl<T> EffectParamType for EffectParamTypeClone<T>
    where T: ShaderParamType,
          <T as ShaderParamType>::RustType: Default,
{
    type ShaderParamType = T;
    type PreparedValueType = <T as ShaderParamType>::RustType;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &GraphicsContext,
    ) -> GraphicsContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType> {
        ContextDependent::new(prepared, context).disable()
    }

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
        context: &'b FilterContext,
    ) {
        param.set_param_value(&staged, context);
    }
}

pub struct EffectParamTypeTexture;

impl EffectParamType for EffectParamTypeTexture {
    type ShaderParamType = ShaderParamTypeTexture;
    type PreparedValueType = TextureDescriptor;

    fn convert_and_stage_value(
        prepared: Self::PreparedValueType,
        context: &GraphicsContext,
    ) -> GraphicsContextDependentDisabled<<Self::ShaderParamType as ShaderParamType>::RustType> {
        let levels: Vec<&[u8]> = prepared.levels.iter().map(|vec| &vec[..]).collect::<Vec<_>>();

        Texture::new(
            prepared.dimensions,
            prepared.color_format,
            &levels,
            prepared.flags,
            context,
        ).disable()
    }

    fn assign_staged_value<'a, 'b>(
        staged: &'b <Self::ShaderParamType as ShaderParamType>::RustType,
        param: &'b mut EnableGuardMut<'a, 'b, GraphicsEffectParamTyped<Self::ShaderParamType>, GraphicsContext>,
        context: &'b FilterContext,
    ) {
        param.set_param_value(&staged, context);
    }
}

/// This type takes care of three different tasks:
/// It stores a _prepared value_ (see `prepare_value`).
/// It creates a graphics resource from the _prepared value_, if it was changed, and stores the result (see `stage_value`).
/// It assigns the staged values to filters (see `assign_value`).
pub struct EffectParam<T: EffectParamType> {
    pub param: GraphicsContextDependentDisabled<GraphicsEffectParamTyped<<T as EffectParamType>::ShaderParamType>>,
    pub prepared_value: Option<T::PreparedValueType>,
    pub staged_value: Option<GraphicsContextDependentDisabled<<<T as EffectParamType>::ShaderParamType as ShaderParamType>::RustType>>,
}

impl<T: EffectParamType> EffectParam<T> {
    pub fn new(param: GraphicsContextDependentDisabled<GraphicsEffectParamTyped<<T as EffectParamType>::ShaderParamType>>) -> Self {
        Self {
            param,
            prepared_value: Some(Default::default()),
            staged_value: None,
        }
    }

    /// Requests a new staged value to be generated from this prepared value
    pub fn prepare_value(&mut self, new_value: T::PreparedValueType) {
        self.prepared_value = Some(new_value);
    }

    /// If a value is prepared (not `None`), creates a graphics resource from that value,
    /// to be used in effect filter processing.
    pub fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        if let Some(prepared_value) = self.prepared_value.take() {
            if let Some(previous) = self.staged_value.replace(<T as EffectParamType>::convert_and_stage_value(
                prepared_value,
                graphics_context,
            )) {
                previous.enable(graphics_context);
            }
        }
    }

    pub fn stage_value_custom<'a>(&mut self, value: GraphicsContextDependentDisabled<<<T as EffectParamType>::ShaderParamType as ShaderParamType>::RustType>, graphics_context: &'a GraphicsContext) {
        if let Some(previous) = self.staged_value.replace(value) {
            previous.enable(graphics_context);
        }
    }

    pub fn take_staged_value(&mut self) -> Option<GraphicsContextDependentDisabled<<<T as EffectParamType>::ShaderParamType as ShaderParamType>::RustType>> {
        self.staged_value.take()
    }

    /// Assigns the staged value to the effect current filter.
    /// Keeps the staged value around.
    pub fn assign_value<'a>(&mut self, context: &'a FilterContext) {
        let staged_value = self.staged_value.as_ref()
            .expect("Tried to assign a value before staging it.")
            .as_enabled(context.graphics());

        <T as EffectParamType>::assign_staged_value(
            &staged_value,
            &mut self.param.as_enabled_mut(context.graphics()),
            context,
        );
    }

    pub fn assign_value_if_staged<'a>(&mut self, context: &'a FilterContext) {
        if self.staged_value.is_some() {
            self.assign_value(context);
        }
    }

    pub fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.param.enable(graphics_context);
        if let Some(staged_value) = self.staged_value {
            staged_value.enable(graphics_context);
        }
    }
}

// A helper trait to ensure most custom effect params follow the same structure.
// Not all custom effect params implement this trait, however.
pub trait EffectParamCustom: BindableProperty + Sized {
    type ShaderParamType: ShaderParamType;
    type PropertyDescriptorSpecialization: PropertyDescriptorSpecialization;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>>;
}

pub struct EffectParamCustomBool {
    pub effect_param: EffectParamBool,
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorBool>
}

impl EffectParamCustom for EffectParamCustomBool {
    type ShaderParamType = ShaderParamTypeBool;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationBool;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: *param.get_param_value_default().unwrap_or(&false),
                default_descriptor_specialization: PropertyDescriptorSpecializationBool {},
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value(property.get_value());

        Ok(Self {
            property,
            effect_param,
        })
    }
}

impl BindableProperty for EffectParamCustomBool {
    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn reload_settings(&mut self, settings: &mut SettingsContext) {
        self.property.reload_settings(settings);
        self.effect_param.prepare_value(self.property.get_value());
    }

    fn prepare_values(&mut self) {}

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub struct EffectParamCustomInt {
    pub effect_param: EffectParamInt,
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorI32>
}

impl EffectParamCustom for EffectParamCustomInt {
    type ShaderParamType = ShaderParamTypeInt;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationI32;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: *param.get_param_value_default().unwrap_or(&0),
                default_descriptor_specialization: Self::PropertyDescriptorSpecialization {
                    min: std::i32::MIN,
                    max: std::i32::MAX,
                    step: 1,
                    slider: false,
                },
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value(property.get_value());

        Ok(Self {
            property,
            effect_param,
        })
    }
}

impl BindableProperty for EffectParamCustomInt {
    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn reload_settings(&mut self, settings: &mut SettingsContext) {
        self.property.reload_settings(settings);
        self.effect_param.prepare_value(self.property.get_value());
    }

    fn prepare_values(&mut self) {}

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub struct EffectParamCustomFloat {
    pub effect_param: EffectParamFloat,
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>
}

impl EffectParamCustom for EffectParamCustomFloat {
    type ShaderParamType = ShaderParamTypeFloat;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationF64;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: *param.get_param_value_default().unwrap_or(&0.0) as f64,
                default_descriptor_specialization: Self::PropertyDescriptorSpecialization {
                    min: std::f64::MIN,
                    max: std::f64::MAX,
                    step: 0.1,
                    slider: false,
                },
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value(property.get_value() as f32);

        Ok(Self {
            property,
            effect_param,
        })
    }
}

impl BindableProperty for EffectParamCustomFloat {
    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn reload_settings(&mut self, settings: &mut SettingsContext) {
        self.property.reload_settings(settings);
        self.effect_param.prepare_value(self.property.get_value() as f32);
    }

    fn prepare_values(&mut self) {}

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub struct EffectParamCustomColor {
    pub effect_param: EffectParamVec4,
    pub property: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorColor>
}

impl EffectParamCustom for EffectParamCustomColor {
    type ShaderParamType = ShaderParamTypeVec4;
    type PropertyDescriptorSpecialization = PropertyDescriptorSpecializationColor;

    fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<Self::ShaderParamType>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: Color(*param.get_param_value_default().unwrap_or(&[0.0; 4])),
                default_descriptor_specialization: Self::PropertyDescriptorSpecialization {},
            },
            identifier,
            None,
            preprocess_result,
            settings,
        )?;
        let mut effect_param = EffectParam::new(param.disable());

        effect_param.prepare_value((property.get_value() as Color).into());

        Ok(Self {
            property,
            effect_param,
        })
    }
}

impl BindableProperty for EffectParamCustomColor {
    fn add_properties(&self, properties: &mut Properties) {
        self.property.add_properties(properties);
    }

    fn reload_settings(&mut self, settings: &mut SettingsContext) {
        self.property.reload_settings(settings);
        self.effect_param.prepare_value((self.property.get_value() as Color).into());
    }

    fn prepare_values(&mut self) {}

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        self.effect_param.assign_value(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        self.effect_param.enable_and_drop(graphics_context);
    }
}

pub struct EffectParamCustomFFT {
    pub effect_param: EffectParamTexture,
    pub effect_param_previous: Option<EffectParamTexture>,
    pub audio_fft: Option<Arc<GlobalStateAudioFFT>>,
    pub property_mix: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorI32>,
    pub property_channel: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorI32>,
    pub property_dampening_factor_attack: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>,
    pub property_dampening_factor_release: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>,
    pub property_mel_enabled: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorBool>,
    pub property_n_mels: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorI32>,
    pub property_f_min: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>,
    pub property_f_max: LoadedValueTypeProperty<LoadedValueTypePropertyDescriptorF64>,
}

// Does not implement EffectParamCustom because of different argument requirements
impl EffectParamCustomFFT {
    pub fn new<'a>(
        param: GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<ShaderParamTypeTexture>>,
        param_previous: Option<GraphicsContextDependentEnabled<'a, GraphicsEffectParamTyped<ShaderParamTypeTexture>>>,
        identifier: &str,
        settings: &mut SettingsContext,
        preprocess_result: &PreprocessResult,
    ) -> Result<Self, Cow<'static, str>> {
        let property_mix = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: false,
                default_value: 1,
                default_descriptor_specialization: PropertyDescriptorSpecializationI32 {
                    min: 1,
                    max: MAX_AUDIO_MIXES as i32,
                    step: 1,
                    slider: false,
                },
            },
            identifier,
            Some("mix"),
            preprocess_result,
            settings,
        )?;
        let property_channel = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: false,
                default_value: 1,
                default_descriptor_specialization: PropertyDescriptorSpecializationI32 {
                    min: 1,
                    max: 2, // FIXME: Causes crashes when `MAX_AUDIO_CHANNELS as i32` is used, supposedly fixed in next OBS release
                    step: 1,
                    slider: false,
                },
            },
            identifier,
            Some("channel"),
            preprocess_result,
            settings,
        )?;
        let property_dampening_factor_attack = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: 0.0,
                default_descriptor_specialization: PropertyDescriptorSpecializationF64 {
                    min: 0.0,
                    max: 100.0,
                    step: 0.01,
                    slider: true,
                },
            },
            identifier,
            Some("dampening_factor_attack"),
            preprocess_result,
            settings,
        )?;
        let property_dampening_factor_release = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: 0.0,
                default_descriptor_specialization: PropertyDescriptorSpecializationF64 {
                    min: 0.0,
                    max: 100.0,
                    step: 0.01,
                    slider: true,
                },
            },
            identifier,
            Some("dampening_factor_release"),
            preprocess_result,
            settings,
        )?;
        let property_mel_enabled = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: false,
                default_descriptor_specialization: PropertyDescriptorSpecializationBool {},
            },
            identifier,
            Some("mel_enabled"),
            preprocess_result,
            settings,
        )?;
        let property_n_mels = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: 128,
                default_descriptor_specialization: PropertyDescriptorSpecializationI32 {
                    min: 1,
                    max: 1024,
                    step: 1,
                    slider: false,
                },
            },
            identifier,
            Some("n_mels"),
            preprocess_result,
            settings,
        )?;
        let property_f_min = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: 0.0,
                default_descriptor_specialization: PropertyDescriptorSpecializationF64 {
                    min: 0.0,
                    max: 10000.0,
                    step: 1.0,
                    slider: false,
                },
            },
            identifier,
            Some("f_min"),
            preprocess_result,
            settings,
        )?;
        let property_f_max = <LoadedValueTypeProperty<_> as LoadedValueType>::from(
            LoadedValueTypePropertyArgs {
                allow_definitions_in_source: true,
                default_value: 10000.0,
                default_descriptor_specialization: PropertyDescriptorSpecializationF64 {
                    min: 0.0,
                    max: 20000.0,
                    step: 1.0,
                    slider: false,
                },
            },
            identifier,
            Some("f_max"),
            preprocess_result,
            settings,
        )?;

        let audio_fft_descriptor = GlobalStateAudioFFTDescriptor::new_with_mel(
            property_mix.get_value() as usize - 1,
            property_channel.get_value() as usize - 1,
            property_dampening_factor_attack.get_value() / 100.0,
            property_dampening_factor_release.get_value() / 100.0,
            WindowFunction::Hanning,
            property_mel_enabled.get_value(),
            property_n_mels.get_value() as usize,
            property_f_min.get_value() as f32,
            property_f_max.get_value() as f32,
        );

        let mut result = Self {
            effect_param: EffectParam::new(param.disable()),
            effect_param_previous: param_previous.map(|param_previous| EffectParam::new(param_previous.disable())),
            audio_fft: None,
            property_mix,
            property_channel,
            property_dampening_factor_attack,
            property_dampening_factor_release,
            property_mel_enabled,
            property_n_mels,
            property_f_min,
            property_f_max,
        };

        result.request_audio_fft();

        Ok(result)
    }

    fn request_audio_fft(&mut self) {
        let audio_fft_descriptor = GlobalStateAudioFFTDescriptor::new_with_mel(
            self.property_mix.get_value() as usize - 1,
            self.property_channel.get_value() as usize - 1,
            self.property_dampening_factor_attack.get_value() / 100.0,
            self.property_dampening_factor_release.get_value() / 100.0,
            WindowFunction::Hanning,
            self.property_mel_enabled.get_value(),
            self.property_n_mels.get_value() as usize,
            self.property_f_min.get_value() as f32,
            self.property_f_max.get_value() as f32,
        );

        self.audio_fft = Some(GLOBAL_STATE.request_audio_fft(&audio_fft_descriptor));
    }
}

impl BindableProperty for EffectParamCustomFFT {
    fn add_properties(&self, properties: &mut Properties) {
        self.property_mix.add_properties(properties);
        self.property_channel.add_properties(properties);
        self.property_dampening_factor_attack.add_properties(properties);
        self.property_dampening_factor_release.add_properties(properties);
        self.property_mel_enabled.add_properties(properties);
        self.property_n_mels.add_properties(properties);
        self.property_f_min.add_properties(properties);
        self.property_f_max.add_properties(properties);
    }

    fn reload_settings(&mut self, settings: &mut SettingsContext) {
        self.property_mix.reload_settings(settings);
        self.property_channel.reload_settings(settings);
        self.property_dampening_factor_attack.reload_settings(settings);
        self.property_dampening_factor_release.reload_settings(settings);
        self.property_mel_enabled.reload_settings(settings);
        self.property_n_mels.reload_settings(settings);
        self.property_f_min.reload_settings(settings);
        self.property_f_max.reload_settings(settings);
        self.request_audio_fft();
    }

    fn prepare_values(&mut self) {
        let fft_result = if let Some(result) = self.audio_fft.as_mut().unwrap().retrieve_result() {
            result
        } else {
            return;
        };
        
        // Check if Mel is enabled in the FFT result
        if self.property_mel_enabled.get_value() && fft_result.mel_spectrum.is_some() {
            // Generate texture from Mel data
            let mel_spectrum = fft_result.mel_spectrum.as_ref().unwrap();
            let texture_data = unsafe {
                std::slice::from_raw_parts::<u8>(
                    mel_spectrum.as_ptr() as *const _,
                    mel_spectrum.len() * std::mem::size_of::<f32>(),
                )
            }.iter().copied().collect::<Vec<_>>();
            
            let texture_mel = TextureDescriptor {
                dimensions: [mel_spectrum.len(), 1],
                color_format: ColorFormatKind::R32F,
                levels: smallvec![texture_data],
                flags: 0,
            };
            
            self.effect_param.prepare_value(texture_mel);
        } else {
            // Use standard FFT data
            let frequency_spectrum = &fft_result.frequency_spectrum;
            let texture_data = unsafe {
                std::slice::from_raw_parts::<u8>(
                    frequency_spectrum.as_ptr() as *const _,
                    frequency_spectrum.len() * std::mem::size_of::<f32>(),
                )
            }.iter().copied().collect::<Vec<_>>();
            
            let texture_fft = TextureDescriptor {
                dimensions: [frequency_spectrum.len(), 1],
                color_format: ColorFormatKind::R32F,
                levels: smallvec![texture_data],
                flags: 0,
            };
            
            self.effect_param.prepare_value(texture_fft);
        }
    }

    fn stage_value<'a>(&mut self, graphics_context: &'a GraphicsContext) {
        if let Some(effect_param_previous) = self.effect_param_previous.as_mut() {
            if let Some(previous_texture_fft) = self.effect_param.take_staged_value() {
                effect_param_previous.stage_value_custom(previous_texture_fft, graphics_context);
            }
        }

        self.effect_param.stage_value(graphics_context);
    }

    fn assign_value<'a>(&mut self, graphics_context: &'a FilterContext) {
        if let Some(effect_param_previous) = self.effect_param_previous.as_mut() {
            effect_param_previous.assign_value_if_staged(graphics_context);
        }
        self.effect_param.assign_value_if_staged(graphics_context);
    }

    fn enable_and_drop(self, graphics_context: &GraphicsContext) {
        if let Some(effect_param_previous) = self.effect_param_previous {
            effect_param_previous.enable_and_drop(graphics_context);
        }
        self.effect_param.enable_and_drop(graphics_context);
    }
}
