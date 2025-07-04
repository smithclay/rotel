use crate::exporters::clickhouse::payload::{ClickhousePayload, ClickhousePayloadBuilder};
use crate::exporters::clickhouse::request_builder::TransformPayload;
use crate::exporters::clickhouse::request_mapper::RequestType;
use crate::exporters::clickhouse::schema::SpanRow;
use crate::exporters::clickhouse::transformer::{
    Transformer, find_attribute, get_scope_properties,
};
use crate::otlp::cvattr;
use opentelemetry_proto::tonic::trace::v1::span::SpanKind;
use opentelemetry_proto::tonic::trace::v1::{ResourceSpans, Span};
use opentelemetry_semantic_conventions::resource::SERVICE_NAME;
use tower::BoxError;

impl TransformPayload<ResourceSpans> for Transformer {
    fn transform(
        &self,
        input: Vec<ResourceSpans>,
    ) -> Result<Vec<(RequestType, ClickhousePayload)>, BoxError> {
        let mut payload_builder = ClickhousePayloadBuilder::new(self.compression.clone());
        for rs in input {
            let res_attrs = rs.resource.unwrap_or_default().attributes;
            let res_attrs = cvattr::convert(&res_attrs);
            let service_name = find_attribute(SERVICE_NAME, &res_attrs);

            for ss in rs.scope_spans {
                let (scope_name, scope_version) = get_scope_properties(ss.scope.as_ref());

                for span in ss.spans {
                    let span_attrs = cvattr::convert(&span.attributes);
                    let status_code = status_code(&span);
                    let status_message = status_message(&span);

                    let row = SpanRow {
                        timestamp: span.start_time_unix_nano,
                        trace_id: hex::encode(span.trace_id),
                        span_id: hex::encode(span.span_id),
                        parent_span_id: hex::encode(span.parent_span_id),
                        trace_state: span.trace_state,
                        span_name: span.name,
                        span_kind: span_kind_to_string(span.kind),
                        service_name: service_name.clone(),
                        resource_attributes: self.transform_attrs(&res_attrs),
                        scope_name: scope_name.clone(),
                        scope_version: scope_version.clone(),
                        span_attributes: self.transform_attrs(&span_attrs),
                        duration: (span.end_time_unix_nano - span.start_time_unix_nano) as i64,
                        status_code,
                        status_message,
                        events_timestamp: span.events.iter().map(|e| e.time_unix_nano).collect(),
                        events_name: span.events.iter().map(|e| e.name.clone()).collect(),
                        events_attributes: span
                            .events
                            .iter()
                            .map(|e| {
                                let event_attrs = cvattr::convert(&e.attributes);
                                self.transform_attrs(&event_attrs)
                            })
                            .collect(),
                        links_trace_id: span
                            .links
                            .iter()
                            .map(|l| hex::encode(&l.trace_id))
                            .collect(),
                        links_span_id: span.links.iter().map(|l| hex::encode(&l.span_id)).collect(),
                        links_trace_state: span
                            .links
                            .iter()
                            .map(|l| l.trace_state.clone())
                            .collect(),
                        links_attributes: span
                            .links
                            .iter()
                            .map(|l| {
                                let link_attrs = cvattr::convert(&l.attributes);
                                self.transform_attrs(&link_attrs)
                            })
                            .collect(),
                    };

                    payload_builder.add_row(&row)?;
                }
            }
        }

        payload_builder
            .finish()
            .map(|payload| vec![(RequestType::Traces, payload)])
    }
}

fn span_kind_to_string(kind: i32) -> String {
    if kind == SpanKind::Internal as i32 {
        "Internal".to_string()
    } else if kind == SpanKind::Server as i32 {
        "Server".to_string()
    } else if kind == SpanKind::Client as i32 {
        "Client".to_string()
    } else if kind == SpanKind::Producer as i32 {
        "Producer".to_string()
    } else if kind == SpanKind::Consumer as i32 {
        "client".to_string()
    } else {
        "Unspecified".to_string()
    }
}

fn status_message(span: &Span) -> String {
    match &span.status {
        None => "".to_string(),
        Some(s) => s.message.clone(),
    }
}

fn status_code(span: &Span) -> String {
    match &span.status {
        None => "Unset".to_string(),
        Some(s) => match s.code {
            1 => "Ok".to_string(),
            2 => "Error".to_string(),
            _ => "Unset".to_string(),
        },
    }
}
