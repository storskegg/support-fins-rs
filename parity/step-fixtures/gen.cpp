// Writes the extra STEP inputs of the oracle corpus (parity/oracle/step.js) with OpenCascade 7.6.1:
// shapes tests/fixtures/*.step do not have -- spheres, cones, tori, fillets, B-spline lofts, booleans,
// a nested named assembly, loose shells and faces, another length unit.
//   usage: gen <out-dir>      (built by tools/build-occt.sh's `gen-step-fixtures` step; see README.md here)
#include <string>
#include <BRepAlgoAPI_Cut.hxx>
#include <BRepBuilderAPI_MakeFace.hxx>
#include <BRepBuilderAPI_MakeWire.hxx>
#include <BRepBuilderAPI_MakeEdge.hxx>
#include <BRepBuilderAPI_Transform.hxx>
#include <BRepFilletAPI_MakeChamfer.hxx>
#include <BRepFilletAPI_MakeFillet.hxx>
#include <BRepOffsetAPI_ThruSections.hxx>
#include <BRepPrimAPI_MakeBox.hxx>
#include <BRepPrimAPI_MakeCone.hxx>
#include <BRepPrimAPI_MakeCylinder.hxx>
#include <BRepPrimAPI_MakeRevol.hxx>
#include <BRepPrimAPI_MakeSphere.hxx>
#include <BRepPrimAPI_MakeTorus.hxx>
#include <BRep_Builder.hxx>
#include <GC_MakeCircle.hxx>
#include <Geom_BSplineCurve.hxx>
#include <GeomAPI_PointsToBSpline.hxx>
#include <Interface_Static.hxx>
#include <STEPCAFControl_Writer.hxx>
#include <STEPControl_Writer.hxx>
#include <TColgp_Array1OfPnt.hxx>
#include <TDF_Label.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Compound.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <gp_Ax2.hxx>
#include <gp_Circ.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>
#include <TCollection_ExtendedString.hxx>

static std::string out;

static void write(const std::string& name, const TopoDS_Shape& s) {
    STEPControl_Writer w;
    Interface_Static::SetCVal("write.step.unit", "MM");
    w.Transfer(s, STEPControl_AsIs);
    w.Write((out + "/" + name + ".step").c_str());
}

static TopoDS_Shape moved(const TopoDS_Shape& s, double x, double y, double z) {
    gp_Trsf t; t.SetTranslation(gp_Vec(x, y, z));
    return BRepBuilderAPI_Transform(s, t, true).Shape();
}

static TDF_Label named(const Handle(XCAFDoc_ShapeTool)& st, const TopoDS_Shape& s, const char* name) {
    TDF_Label l = st->AddShape(s, false);
    TDataStd_Name::Set(l, TCollection_ExtendedString(name));
    return l;
}

int main(int argc, char** argv) {
    out = argc > 1 ? argv[1] : ".";

    write("sphere", BRepPrimAPI_MakeSphere(12.0).Shape());
    write("cone", BRepPrimAPI_MakeCone(10.0, 3.0, 25.0).Shape());
    write("torus", BRepPrimAPI_MakeTorus(15.0, 4.0).Shape());

    TopoDS_Shape cyl = BRepPrimAPI_MakeCylinder(9.0, 20.0).Shape();
    TopoDS_Shape hole = BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, -1), gp_Dir(0, 0, 1)), 3.5, 22.0).Shape();
    write("tube", BRepAlgoAPI_Cut(cyl, hole).Shape());

    {   // a box with every vertical edge filleted and the top edges chamfered
        TopoDS_Shape box = BRepPrimAPI_MakeBox(30.0, 20.0, 12.0).Shape();
        BRepFilletAPI_MakeFillet f(box);
        for (TopExp_Explorer ex(box, TopAbs_EDGE); ex.More(); ex.Next()) {
            const TopoDS_Edge& e = TopoDS::Edge(ex.Current());
            gp_Pnt a = BRep_Tool::Pnt(TopExp::FirstVertex(e)), b = BRep_Tool::Pnt(TopExp::LastVertex(e));
            if (fabs(a.X() - b.X()) < 1e-9 && fabs(a.Y() - b.Y()) < 1e-9) f.Add(3.0, e);
        }
        write("rounded-box", f.Shape());
    }

    {   // a loft through three circles (B-spline surface)
        BRepOffsetAPI_ThruSections loft(true, false);
        double r[3] = {10, 6, 9}, z[3] = {0, 14, 30};
        for (int i = 0; i < 3; i++) {
            gp_Circ c(gp_Ax2(gp_Pnt(0, 0, z[i]), gp_Dir(0, 0, 1)), r[i]);
            loft.AddWire(BRepBuilderAPI_MakeWire(BRepBuilderAPI_MakeEdge(c).Edge()).Wire());
        }
        loft.Build();
        write("loft", loft.Shape());
    }

    {   // a vase: a spline profile revolved
        TColgp_Array1OfPnt pts(1, 6);
        pts(1) = gp_Pnt(0, 0, 0); pts(2) = gp_Pnt(8, 0, 1); pts(3) = gp_Pnt(11, 0, 9);
        pts(4) = gp_Pnt(6, 0, 18); pts(5) = gp_Pnt(7.5, 0, 26); pts(6) = gp_Pnt(0, 0, 27);
        Handle(Geom_BSplineCurve) c = GeomAPI_PointsToBSpline(pts).Curve();
        TopoDS_Edge e = BRepBuilderAPI_MakeEdge(c).Edge();
        TopoDS_Edge axis = BRepBuilderAPI_MakeEdge(gp_Pnt(0, 0, 27), gp_Pnt(0, 0, 0)).Edge();
        TopoDS_Wire w = BRepBuilderAPI_MakeWire(e, axis).Wire();
        TopoDS_Face face = BRepBuilderAPI_MakeFace(w).Face();
        write("vase", BRepPrimAPI_MakeRevol(face, gp_Ax1(gp_Pnt(0, 0, 0), gp_Dir(0, 0, 1))).Shape());
    }

    {   // a box with a spherical pocket: intersection curves between a plane and a sphere
        TopoDS_Shape box = BRepPrimAPI_MakeBox(24.0, 24.0, 10.0).Shape();
        TopoDS_Shape ball = BRepPrimAPI_MakeSphere(gp_Pnt(12, 12, 10), 8.0).Shape();
        write("pocket", BRepAlgoAPI_Cut(box, ball).Shape());
    }

    {   // an open shell (a box without its lid) and a loose face, as separate pieces of one compound
        TopoDS_Shape box = BRepPrimAPI_MakeBox(15.0, 15.0, 8.0).Shape();
        BRep_Builder bb;
        TopoDS_Shell shell; bb.MakeShell(shell);
        int n = 0;
        for (TopExp_Explorer ex(box, TopAbs_FACE); ex.More(); ex.Next()) { if (++n != 6) bb.Add(shell, ex.Current()); }
        TopoDS_Face loose = BRepBuilderAPI_MakeFace(gp_Pln(gp_Pnt(40, 0, 0), gp_Dir(0, 0, 1)), 0, 10, 0, 10).Face();
        TopoDS_Compound c; bb.MakeCompound(c); bb.Add(c, shell); bb.Add(c, loose);
        write("shell-and-face", c);
    }

    {   // a solid plus a loose shell side by side
        TopoDS_Shape solid = BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape();
        TopoDS_Shape other = moved(BRepPrimAPI_MakeSphere(6.0).Shape(), 30, 0, 0);
        BRep_Builder bb; TopoDS_Compound c; bb.MakeCompound(c); bb.Add(c, solid); bb.Add(c, other);
        write("compound-two-solids", c);
    }

    {   // a file in inches: 1 x 2 x 0.5 in must come back as 25.4 x 50.8 x 12.7 mm
        STEPControl_Writer w;
        Interface_Static::SetCVal("write.step.unit", "IN");
        w.Transfer(BRepPrimAPI_MakeBox(1.0, 2.0, 0.5).Shape(), STEPControl_AsIs);
        w.Write((out + "/inch-box.step").c_str());
    }

    {   // a named, nested assembly: Asm { Frame (box), Pair { Wheel x2 (one definition, two placements) }, Lone sphere }
        Handle(TDocStd_Document) doc = new TDocStd_Document("XmlXCAF");
        Handle(XCAFDoc_ShapeTool) st = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
        TDF_Label frame = named(st, BRepPrimAPI_MakeBox(40.0, 20.0, 6.0).Shape(), "Frame");
        TDF_Label wheel = named(st, BRepPrimAPI_MakeCylinder(gp_Ax2(gp_Pnt(0, 0, 0), gp_Dir(0, 1, 0)), 7.0, 4.0).Shape(), "Wheel");
        TDF_Label lone = named(st, BRepPrimAPI_MakeSphere(5.0).Shape(), "Lone");
        TDF_Label pair = st->NewShape();
        TDataStd_Name::Set(pair, TCollection_ExtendedString("Pair"));
        gp_Trsf t1; t1.SetTranslation(gp_Vec(8, -5, 3));
        gp_Trsf t2; t2.SetTranslation(gp_Vec(32, -5, 3));
        st->AddComponent(pair, wheel, TopLoc_Location(t1));
        st->AddComponent(pair, wheel, TopLoc_Location(t2));
        TDF_Label asm_ = st->NewShape();
        TDataStd_Name::Set(asm_, TCollection_ExtendedString("Asm"));
        st->AddComponent(asm_, frame, TopLoc_Location());
        st->AddComponent(asm_, pair, TopLoc_Location());
        gp_Trsf t3; t3.SetTranslation(gp_Vec(60, 10, 5));
        st->AddComponent(asm_, lone, TopLoc_Location(t3));
        st->UpdateAssemblies();
        STEPCAFControl_Writer w;
        w.SetNameMode(true);
        w.Transfer(doc, STEPControl_AsIs);
        w.Write((out + "/assembly.step").c_str());
    }

    {   // two top-level free shapes, named: the root branches at once
        Handle(TDocStd_Document) doc = new TDocStd_Document("XmlXCAF");
        Handle(XCAFDoc_ShapeTool) st = XCAFDoc_DocumentTool::ShapeTool(doc->Main());
        named(st, BRepPrimAPI_MakeCone(8.0, 0.0, 16.0).Shape(), "Cone A");
        named(st, moved(BRepPrimAPI_MakeTorus(9.0, 2.5).Shape(), 30, 0, 0), "Ring B");
        named(st, moved(BRepPrimAPI_MakeBox(10.0, 10.0, 10.0).Shape(), 0, 30, 0), "");
        STEPCAFControl_Writer w;
        w.SetNameMode(true);
        w.Transfer(doc, STEPControl_AsIs);
        w.Write((out + "/three-roots.step").c_str());
    }
    return 0;
}
