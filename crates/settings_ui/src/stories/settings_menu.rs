use gpui::View;

use ui::prelude::*;

use crate::{
    SecondarySettingType, SettingLayout, SettingType, SettingsGroup, SettingsItem, SettingsMenu,
    ToggleType,
};

pub struct SettingsMenuStory {
    menus: Vec<(SharedString, View<SettingsMenu>)>,
}

impl SettingsMenuStory {
    pub fn new() -> Self {
        Self { menus: Vec::new() }
    }

    pub fn init(cx: &mut ViewContext<Self>) -> Self {
        let mut story = Self::new();
        story.empty_menu(cx);
        story.menu_single_group(cx);
        story.editor_example(cx);
        story
    }
}

impl SettingsMenuStory {
    pub fn empty_menu(&mut self, cx: &mut ViewContext<Self>) {
        let menu = cx.new_view(|_cx| SettingsMenu::new("Empty Menu"));

        self.menus.push(("Empty Menu".into(), menu));
    }

    pub fn menu_single_group(&mut self, cx: &mut ViewContext<Self>) {
        let theme_setting = SettingsItem::new(
            "theme-setting",
            "Theme".into(),
            SettingType::Dropdown,
            Some(cx.theme().name.clone().into()),
        )
        .layout(SettingLayout::Stacked);
        let high_contrast_setting = SettingsItem::new(
            "theme-contrast",
            "Use high contrast theme".into(),
            SettingType::Toggle(ToggleType::Checkbox),
            Some(true.into()),
        );
        let appearance_setting = SettingsItem::new(
            "switch-appearance",
            "Match system appearance".into(),
            SettingType::ToggleAnd(SecondarySettingType::Dropdown),
            Some("When Dark".to_string().into()),
        )
        .layout(SettingLayout::FullLineJustified);

        let group = SettingsGroup::new("Appearance")
            .add_setting(theme_setting)
            .add_setting(appearance_setting)
            .add_setting(high_contrast_setting);

        let menu = cx.new_view(|_cx| SettingsMenu::new("Appearance").add_group(group));

        self.menus.push(("Single Group".into(), menu));
    }

    pub fn editor_example(&mut self, cx: &mut ViewContext<Self>) {
        let font_group = SettingsGroup::new("Font").add_setting(
            SettingsItem::new(
                "enable-ligatures",
                "Enable Ligatures".into(),
                SettingType::Toggle(ToggleType::Checkbox),
                None,
            )
            .toggled(true),
        );

        let editor_group = SettingsGroup::new("Editor")
            .add_setting(
                SettingsItem::new(
                    "show-indent-guides",
                    "Indent Guides".into(),
                    SettingType::Toggle(ToggleType::Checkbox),
                    None,
                )
                .toggled(true),
            )
            .add_setting(
                SettingsItem::new(
                    "show-git-blame",
                    "Git Blame".into(),
                    SettingType::Toggle(ToggleType::Checkbox),
                    None,
                )
                .toggled(false),
            );

        let gutter_group = SettingsGroup::new("Gutter")
            .add_setting(
                SettingsItem::new(
                    "enable-git-hunks",
                    "Show Git Hunks".into(),
                    SettingType::Toggle(ToggleType::Checkbox),
                    None,
                )
                .toggled(true),
            )
            .add_setting(
                SettingsItem::new(
                    "show-line-numbers",
                    "Line Numbers".into(),
                    SettingType::ToggleAnd(SecondarySettingType::Dropdown),
                    Some("Ascending".to_string().into()),
                )
                .toggled(true),
            );

        let scrollbar_group = SettingsGroup::new("Scrollbar")
            .add_setting(
                SettingsItem::new(
                    "scrollbar-visibility",
                    "Show scrollbar when:".into(),
                    SettingType::Dropdown,
                    Some("Always Visible".to_string().into()),
                )
                .layout(SettingLayout::FullLine)
                .hide_label(true),
            )
            .add_setting(
                SettingsItem::new(
                    "show-diagnostic-markers",
                    "Diagnostic Markers".into(),
                    SettingType::Toggle(ToggleType::Checkbox),
                    None,
                )
                .toggled(true),
            )
            .add_setting(
                SettingsItem::new(
                    "show-git-markers",
                    "Git Status Markers".into(),
                    SettingType::Toggle(ToggleType::Checkbox),
                    None,
                )
                .toggled(false),
            )
            .add_setting(
                SettingsItem::new(
                    "show-selection-markers",
                    "Selection & Match Markers".into(),
                    SettingType::Toggle(ToggleType::Checkbox),
                    None,
                )
                .toggled(true),
            );

        let menu = cx.new_view(|_cx| {
            SettingsMenu::new("Editor")
                .add_group(font_group)
                .add_group(editor_group)
                .add_group(gutter_group)
                .add_group(scrollbar_group)
        });

        self.menus.push(("Editor Example".into(), menu));
    }
}

impl Render for SettingsMenuStory {
    fn render(&mut self, cx: &mut ViewContext<Self>) -> impl IntoElement {
        div()
            .bg(cx.theme().colors().background)
            .text_color(cx.theme().colors().text)
            .children(self.menus.iter().map(|(name, menu)| {
                v_flex()
                    .p_2()
                    .gap_2()
                    .child(Headline::new(name.clone()).size(HeadlineSize::Medium))
                    .child(menu.clone())
            }))
    }
}