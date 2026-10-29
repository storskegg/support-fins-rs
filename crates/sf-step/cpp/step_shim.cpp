// The native twin of occt-import-js 0.0.23's ReadStepFile (occt-import-js/src/importer-step.cpp,
// importer-xcaf.cpp, importer-utils.cpp, js-interface.cpp at kovacsv/occt-import-js@0.0.23), built
// on the same OpenCascade (7.6.1, commit d2abb6d8). It is that code with the JS-object plumbing
// replaced by plain vectors behind a small C API (step_shim.h); colours, normals and brep_faces are
// dropped because web/stepworker.js deletes the normals and brep_faces and nothing reads the colours.
//
// Keep it a transliteration: same traversal, same order, same OCCT calls, so the triangles are the
// triangles the web gets.

#include "step_shim.h"

#include <cstdint>
#include <cstring>
#include <functional>
#include <memory>
#include <streambuf>
#include <string>
#include <vector>

#include <BRepMesh_IncrementalMesh.hxx>
#include <BRep_Tool.hxx>
#include <IFSelect_ReturnStatus.hxx>
#include <Poly_Triangulation.hxx>
#include <STEPCAFControl_Reader.hxx>
#include <STEPControl_Reader.hxx>
#include <TDF_ChildIterator.hxx>
#include <TDF_Label.hxx>
#include <TDF_LabelSequence.hxx>
#include <TDataStd_Name.hxx>
#include <TDocStd_Document.hxx>
#include <TopExp_Explorer.hxx>
#include <TopoDS.hxx>
#include <TopoDS_Face.hxx>
#include <TopoDS_Shape.hxx>
#include <TopLoc_Location.hxx>
#include <UnitsMethods.hxx>
#include <XCAFDoc_DocumentTool.hxx>
#include <XCAFDoc_ShapeTool.hxx>
#include <gp_Pnt.hxx>
#include <gp_Trsf.hxx>

namespace {

struct Mesh {
    std::string name;
    std::vector<double> positions;
    std::vector<uint32_t> indices;
};

struct Node {
    std::string name;
    std::vector<int> meshes;
    std::vector<int> children;
};

struct VectorBuffer : std::streambuf {
    VectorBuffer(const uint8_t* data, size_t len) {
        setg((char*)data, (char*)data, (char*)(data + len));
    }
};

std::string GetLabelNameNoRef(const TDF_Label& label) {
    Handle(TDataStd_Name) nameAttribute = new TDataStd_Name();
    if (!label.FindAttribute(nameAttribute->GetID(), nameAttribute)) {
        return std::string();
    }
    Standard_Integer utf8NameLength = nameAttribute->Get().LengthOfCString();
    char* nameBuf = new char[utf8NameLength + 1];
    nameAttribute->Get().ToUTF8CString(nameBuf);
    std::string name(nameBuf, utf8NameLength);
    delete[] nameBuf;
    return name;
}

std::string GetLabelName(const TDF_Label& label, const Handle(XCAFDoc_ShapeTool)& shapeTool) {
    if (XCAFDoc_ShapeTool::IsReference(label)) {
        TDF_Label referredShapeLabel;
        shapeTool->GetReferredShape(label, referredShapeLabel);
        return GetLabelName(referredShapeLabel, shapeTool);
    }
    return GetLabelNameNoRef(label);
}

std::string GetShapeName(const TopoDS_Shape& shape, const Handle(XCAFDoc_ShapeTool)& shapeTool) {
    TDF_Label shapeLabel;
    if (!shapeTool->Search(shape, shapeLabel)) {
        return std::string();
    }
    return GetLabelName(shapeLabel, shapeTool);
}

bool IsFreeShape(const TDF_Label& label, const Handle(XCAFDoc_ShapeTool)& shapeTool) {
    TopoDS_Shape tmpShape;
    return shapeTool->GetShape(label, tmpShape) && shapeTool->IsFree(label);
}

// importer-utils.cpp TriangulateShape, AbsoluteValue branch (web/step.js STEP_PARAMS).
void TriangulateShape(TopoDS_Shape& shape, double linDeflection, double angDeflection) {
    BRepMesh_IncrementalMesh mesh(shape, linDeflection, Standard_False, angDeflection);
}

// OcctFace::EnumerateVertices / EnumerateTriangles (importer-utils.cpp), appended to a mesh.
void AppendFace(const TopoDS_Face& face, Mesh& mesh) {
    TopLoc_Location location;
    Handle(Poly_Triangulation) triangulation = BRep_Tool::Triangulation(face, location);
    if (triangulation.IsNull() || triangulation->NbNodes() == 0 || triangulation->NbTriangles() == 0) {
        return;
    }
    uint32_t vertexOffset = (uint32_t)(mesh.positions.size() / 3);
    gp_Trsf transformation = location.Transformation();
    for (Standard_Integer nodeIndex = 1; nodeIndex <= triangulation->NbNodes(); nodeIndex++) {
        gp_Pnt vertex = triangulation->Node(nodeIndex);
        vertex.Transform(transformation);
        mesh.positions.push_back(vertex.X());
        mesh.positions.push_back(vertex.Y());
        mesh.positions.push_back(vertex.Z());
    }
    bool isReversed = (face.Orientation() == TopAbs_REVERSED);
    for (Standard_Integer triangleIndex = 1; triangleIndex <= triangulation->NbTriangles(); triangleIndex++) {
        Poly_Triangle triangle = triangulation->Triangle(triangleIndex);
        int a = triangle(1) - 1, b = triangle(2) - 1, c = triangle(3) - 1;
        if (isReversed) {
            mesh.indices.push_back(vertexOffset + a);
            mesh.indices.push_back(vertexOffset + c);
            mesh.indices.push_back(vertexOffset + b);
        } else {
            mesh.indices.push_back(vertexOffset + a);
            mesh.indices.push_back(vertexOffset + b);
            mesh.indices.push_back(vertexOffset + c);
        }
    }
}

} // namespace

struct SfStep {
    std::vector<Node> nodes;
    std::vector<Mesh> meshes;
    std::string error;

    Handle(TDocStd_Document) document;
    Handle(XCAFDoc_ShapeTool) shapeTool;
    double linDeflection = 0.01;
    double angDeflection = 0.1;

    // XcafNode::IsMeshNode
    bool IsMeshNode(const TDF_Label& label) const {
        if (!label.HasChild()) return true;
        for (TDF_ChildIterator it(label); it.More(); it.Next()) {
            if (shapeTool->IsSubShape(it.Value())) return true;
        }
        for (TDF_ChildIterator it(label); it.More(); it.Next()) {
            if (IsFreeShape(it.Value(), shapeTool)) return false;
        }
        return true;
    }

    // XcafNode::EnumerateShapeMeshes
    void EnumerateShapeMeshes(const TopoDS_Shape& shape, std::vector<int>& nodeMeshes) {
        auto emit = [&](const TopoDS_Shape& s, bool standalone) {
            Mesh mesh;
            mesh.name = standalone ? std::string() : GetShapeName(s, shapeTool);
            if (standalone) {
                for (TopExp_Explorer ex(s, TopAbs_FACE, TopAbs_SHELL); ex.More(); ex.Next()) {
                    AppendFace(TopoDS::Face(ex.Current()), mesh);
                }
            } else {
                for (TopExp_Explorer ex(s, TopAbs_FACE); ex.More(); ex.Next()) {
                    AppendFace(TopoDS::Face(ex.Current()), mesh);
                }
            }
            meshes.push_back(std::move(mesh));
            nodeMeshes.push_back((int)meshes.size() - 1);
        };
        for (TopExp_Explorer ex(shape, TopAbs_SOLID); ex.More(); ex.Next()) {
            emit(ex.Current(), false);
        }
        for (TopExp_Explorer ex(shape, TopAbs_SHELL, TopAbs_SOLID); ex.More(); ex.Next()) {
            emit(ex.Current(), false);
        }
        TopExp_Explorer standalone(shape, TopAbs_FACE, TopAbs_SHELL);
        if (standalone.More()) {
            emit(shape, true);
        }
    }

    // js-interface.cpp HierarchyWriter::WriteNode over XcafNode / XcafRootNode. Returns the node's index.
    int WriteNode(const TDF_Label* label) {
        int index = (int)nodes.size();
        nodes.emplace_back();
        std::vector<int> nodeMeshes;
        std::vector<TDF_Label> childLabels;
        if (label == nullptr) {
            // XcafRootNode::GetChildren: every free shape under the main label, meshed first
            TDF_Label mainLabel = shapeTool->Label();
            for (TDF_ChildIterator it(mainLabel); it.More(); it.Next()) {
                TDF_Label childLabel = it.Value();
                if (IsFreeShape(childLabel, shapeTool)) {
                    TopoDS_Shape shape = shapeTool->GetShape(childLabel);
                    // (TriangulateShape returns false on a void bounding box only for the ratio type)
                    TriangulateShape(shape, linDeflection, angDeflection);
                    childLabels.push_back(childLabel);
                }
            }
        } else {
            nodes[index].name = GetLabelName(*label, shapeTool);
            if (IsMeshNode(*label)) {
                // WriteMeshes runs before GetChildren, which is empty for a mesh node
                TopoDS_Shape shape = shapeTool->GetShape(*label);
                EnumerateShapeMeshes(shape, nodeMeshes);
            } else {
                for (TDF_ChildIterator it(*label); it.More(); it.Next()) {
                    if (IsFreeShape(it.Value(), shapeTool)) childLabels.push_back(it.Value());
                }
            }
        }
        nodes[index].meshes = nodeMeshes;
        std::vector<int> kids;
        for (const TDF_Label& c : childLabels) {
            kids.push_back(WriteNode(&c));
        }
        nodes[index].children = kids;
        return index;
    }

    // ImporterXcaf::LoadFile + ImporterStep::TransferToDocument
    bool Load(const uint8_t* data, size_t len) {
        document = new TDocStd_Document("XmlXCAF");
        XCAFDoc_DocumentTool::SetLengthUnit(document, 1.0, UnitsMethods_LengthUnit_Millimeter);

        STEPCAFControl_Reader stepCafReader;
        stepCafReader.SetColorMode(true);
        stepCafReader.SetNameMode(true);
        STEPControl_Reader& stepReader = stepCafReader.ChangeReader();
        std::string dummyFileName = "stp";
        VectorBuffer inputBuffer(data, len);
        std::istream inputStream(&inputBuffer);
        IFSelect_ReturnStatus readStatus = stepReader.ReadStream(dummyFileName.c_str(), inputStream);
        if (readStatus != IFSelect_RetDone) return false;
        if (!stepCafReader.Transfer(document)) return false;

        TDF_Label mainLabel = document->Main();
        shapeTool = XCAFDoc_DocumentTool::ShapeTool(mainLabel);
        TDF_LabelSequence labels;
        shapeTool->GetFreeShapes(labels);
        if (labels.IsEmpty()) return false;

        WriteNode(nullptr);
        return true;
    }
};

extern "C" {

SfStep* sf_step_read(const uint8_t* data, size_t len, double linear_deflection, double angular_deflection) {
    SfStep* r = new SfStep();
    r->linDeflection = linear_deflection;
    r->angDeflection = angular_deflection;
    bool ok = false;
    try {
        ok = r->Load(data, len);
    } catch (...) {
        ok = false;
    }
    if (!ok) {
        delete r;
        return nullptr;
    }
    return r;
}

void sf_step_free(SfStep* r) { delete r; }

size_t sf_step_node_count(const SfStep* r) { return r->nodes.size(); }
const char* sf_step_node_name(const SfStep* r, size_t i) { return r->nodes[i].name.c_str(); }
size_t sf_step_node_mesh_count(const SfStep* r, size_t i) { return r->nodes[i].meshes.size(); }
size_t sf_step_node_mesh(const SfStep* r, size_t i, size_t j) { return (size_t)r->nodes[i].meshes[j]; }
size_t sf_step_node_child_count(const SfStep* r, size_t i) { return r->nodes[i].children.size(); }
size_t sf_step_node_child(const SfStep* r, size_t i, size_t j) { return (size_t)r->nodes[i].children[j]; }

size_t sf_step_mesh_count(const SfStep* r) { return r->meshes.size(); }
const char* sf_step_mesh_name(const SfStep* r, size_t i) { return r->meshes[i].name.c_str(); }
size_t sf_step_mesh_vertex_count(const SfStep* r, size_t i) { return r->meshes[i].positions.size() / 3; }
const double* sf_step_mesh_positions(const SfStep* r, size_t i) { return r->meshes[i].positions.data(); }
size_t sf_step_mesh_index_count(const SfStep* r, size_t i) { return r->meshes[i].indices.size(); }
const uint32_t* sf_step_mesh_indices(const SfStep* r, size_t i) { return r->meshes[i].indices.data(); }

}
