import XCTest

/// STEP import through the real window (M6): a `.step` opened from the Open panel is read through OpenCascade, the
/// window opens, and the Status block carries the page's import note ("STEP: tessellated at 0.01 mm."). OpenCascade runs
/// in the StepReader XPC service (X-01), so a file that crashes it is an error in the window, and the app lives on. Run
/// by the orchestrator, like `FinsLoopUITests` (it drives the Open panel); the launch and the window are
/// `PartWindowUITestCase`'s.
///
/// Elements are found by the accessibility identifiers the app sets: `fins-toggle`, `load-status`.
final class StepImportUITests: PartWindowUITestCase {
    func testOpenAStepFile() throws {
        let part = dir.appending(path: "ball.step")
        try Self.sphereStep.write(to: part, atomically: true, encoding: .utf8)
        // the panel offers .step files; OpenCascade takes a moment
        try openPart(part, timeout: 30)
        let note = partWindow.staticTexts["STEP: tessellated at 0.01 mm."]
        XCTAssertTrue(note.waitForExistence(timeout: 10), "the import note")
    }

    /// `loft.step` without one line (`crates/sf-step-xpc/tests/segv/step_segv_loft.step`): OpenCascade dereferences
    /// null reading it, which killed the app before X-01. Now the service's process ends, the window says it could not
    /// read the file, and the app still opens the next one.
    func testABadStepFileIsAnErrorAndTheAppLivesOn() throws {
        let bundled = try XCTUnwrap(Bundle(for: Self.self).url(forResource: "step_segv_loft", withExtension: "step"))
        let part = dir.appending(path: "broken.step")
        try FileManager.default.copyItem(at: bundled, to: part)
        try openPart(part, timeout: 30)
        let status = element("load-status")
        XCTAssertTrue(status.waitForExistence(timeout: 30), "the read failure")
        XCTAssertEqual(text(status), "Could not read broken.step:\nCouldn't read this STEP file")
        XCTAssertEqual(app.state, .runningForeground, "the app survived")

        // the service comes back: the window's Import button opens a good STEP file in the same window
        let ball = dir.appending(path: "ball.step")
        try Self.sphereStep.write(to: ball, atomically: true, encoding: .utf8)
        let importButton = element("empty-import")
        XCTAssertTrue(importButton.waitForExistence(timeout: 5), "the empty window offers Import")
        importButton.click()
        XCTAssertTrue(panelExists("open-panel", timeout: 10), "the Open panel")
        chooseInPanel(ball.path)
        let note = partWindow.staticTexts["STEP: tessellated at 0.01 mm."]
        XCTAssertTrue(note.waitForExistence(timeout: 30), "the good file's import note")
        XCTAssertFalse(status.exists, "the failure is gone")
    }

    /// A 12 mm ball: one spherical face (OpenCascade's own export, `parity/step-fixtures/sphere.step`).
    private static let sphereStep = #"""
ISO-10303-21;
HEADER;
FILE_DESCRIPTION(('Open CASCADE Model'),'2;1');
FILE_NAME('Open CASCADE Shape Model','2026-10-28T18:29:00',('Author'),(
    'Open CASCADE'),'Open CASCADE STEP processor 7.6','Open CASCADE 7.6'
  ,'Unknown');
FILE_SCHEMA(('AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }'));
ENDSEC;
DATA;
#1 = APPLICATION_PROTOCOL_DEFINITION('international standard',
  'automotive_design',2000,#2);
#2 = APPLICATION_CONTEXT(
  'core data for automotive mechanical design processes');
#3 = SHAPE_DEFINITION_REPRESENTATION(#4,#10);
#4 = PRODUCT_DEFINITION_SHAPE('','',#5);
#5 = PRODUCT_DEFINITION('design','',#6,#9);
#6 = PRODUCT_DEFINITION_FORMATION('','',#7);
#7 = PRODUCT('Open CASCADE STEP translator 7.6 1',
  'Open CASCADE STEP translator 7.6 1','',(#8));
#8 = PRODUCT_CONTEXT('',#2,'mechanical');
#9 = PRODUCT_DEFINITION_CONTEXT('part definition',#2,'design');
#10 = ADVANCED_BREP_SHAPE_REPRESENTATION('',(#11,#15),#27);
#11 = AXIS2_PLACEMENT_3D('',#12,#13,#14);
#12 = CARTESIAN_POINT('',(0.,0.,0.));
#13 = DIRECTION('',(0.,0.,1.));
#14 = DIRECTION('',(1.,0.,-0.));
#15 = MANIFOLD_SOLID_BREP('',#16);
#16 = CLOSED_SHELL('',(#17));
#17 = ADVANCED_FACE('',(#18),#22,.T.);
#18 = FACE_BOUND('',#19,.T.);
#19 = VERTEX_LOOP('',#20);
#20 = VERTEX_POINT('',#21);
#21 = CARTESIAN_POINT('',(7.347880794884E-16,-1.799711739194E-31,-12.));
#22 = SPHERICAL_SURFACE('',#23,12.);
#23 = AXIS2_PLACEMENT_3D('',#24,#25,#26);
#24 = CARTESIAN_POINT('',(0.,0.,0.));
#25 = DIRECTION('',(0.,0.,1.));
#26 = DIRECTION('',(1.,0.,-0.));
#27 = ( GEOMETRIC_REPRESENTATION_CONTEXT(3) 
GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#31)) GLOBAL_UNIT_ASSIGNED_CONTEXT(
(#28,#29,#30)) REPRESENTATION_CONTEXT('Context #1',
  '3D Context with UNIT and UNCERTAINTY') );
#28 = ( LENGTH_UNIT() NAMED_UNIT(*) SI_UNIT(.MILLI.,.METRE.) );
#29 = ( NAMED_UNIT(*) PLANE_ANGLE_UNIT() SI_UNIT($,.RADIAN.) );
#30 = ( NAMED_UNIT(*) SI_UNIT($,.STERADIAN.) SOLID_ANGLE_UNIT() );
#31 = UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-07),#28,
  'distance_accuracy_value','confusion accuracy');
#32 = PRODUCT_RELATED_PRODUCT_CATEGORY('part',$,(#7));
ENDSEC;
END-ISO-10303-21;
"""#
}
