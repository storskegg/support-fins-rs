/// Every feature of the window. Adding a feature means adding its folder and one line here: the window and the
/// menu bar compose their contributions from this list. The order is the order of ties (equal `order`) and of the
/// modifiers; where things sit on screen is each contribution's own `order`.
@MainActor
enum Features {
    static let all: [any Feature.Type] = [
        OpenFeature.self,
        BuildSpinnerFeature.self,
        MoveFeature.self,
        RotateFeature.self,
        ScaleFeature.self,
        SuggestFeature.self,
        StrengthFeature.self,
        ViewFeature.self,
        SectionFeature.self,
        RemoveFinsFeature.self,
        DrawFeature.self,
        PlatesFeature.self,
        ExportFeature.self,
        ReadoutFeature.self,
        HelpFeature.self,
        WhatsNewFeature.self,
        CalibrationFeature.self,
        SearchFeature.self,
        ReportFeature.self,
        SessionFeature.self,
    ]
}
