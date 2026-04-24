use rango_server::retrieval::adapters::{
    AdapterCapabilities, GraphRetrievalAdapter, VectorRetrievalAdapter,
};

#[test]
fn retrieval_adapters_are_external_capability_interfaces() {
    fn assert_vector_adapter<T: VectorRetrievalAdapter>() {}
    fn assert_graph_adapter<T: GraphRetrievalAdapter>() {}

    assert_vector_adapter::<AdapterCapabilities>();
    assert_graph_adapter::<AdapterCapabilities>();
}
