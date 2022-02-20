use super::{
    device_types::device_type_add_root_node,
    objects::{self, *},
    sdm::dev_att::DevAttDataFetcher,
    system_model::descriptor::cluster_descriptor_new,
};
use crate::{
    error::*,
    fabric::FabricMgr,
    interaction_model::{
        command::{self, CommandReq, InvokeRespIb},
        core::IMStatusCode,
        messages::{attr_path, attr_response},
        CmdPathIb, InteractionConsumer, Transaction,
    },
    tlv::TLVElement,
    tlv_common::TagType,
    tlv_writer::TLVWriter,
};
use log::{error, info};
use std::sync::{Arc, RwLock};

pub struct DataModel {
    pub node: Arc<RwLock<Box<Node>>>,
}

impl DataModel {
    pub fn new(
        dev_att: Box<dyn DevAttDataFetcher>,
        fabric_mgr: Arc<FabricMgr>,
    ) -> Result<Self, Error> {
        let dm = DataModel {
            node: Arc::new(RwLock::new(Node::new()?)),
        };
        {
            let mut node = dm.node.write()?;
            node.set_changes_cb(Box::new(dm.clone()));
            device_type_add_root_node(&mut node, dev_att, fabric_mgr)?;
        }
        Ok(dm)
    }

    fn handle_command(&self, mut cmd_req: CommandReq) -> Result<(), IMStatusCode> {
        let mut node = self.node.write().unwrap();
        node.get_endpoint(cmd_req.endpoint.into())
            .map_err(|_| IMStatusCode::UnsupportedEndpoint)?
            .get_cluster(cmd_req.cluster)
            .map_err(|_| IMStatusCode::UnsupportedCluster)?
            .handle_command(&mut cmd_req)
    }
}

impl Clone for DataModel {
    fn clone(&self) -> Self {
        DataModel {
            node: self.node.clone(),
        }
    }
}

impl objects::ChangeConsumer for DataModel {
    fn endpoint_added(&self, id: u16, endpoint: &mut Endpoint) -> Result<(), Error> {
        endpoint.add_cluster(cluster_descriptor_new(id, self.clone())?)?;
        Ok(())
    }
}

impl InteractionConsumer for DataModel {
    fn consume_read_attr(
        &self,
        attr_list: TLVElement,
        fab_scoped: bool,
        tw: &mut TLVWriter,
    ) -> Result<(), Error> {
        if fab_scoped {
            error!("Fabric scoped attribute read not yet supported");
        }
        let attr_list = attr_list
            .confirm_array()?
            .iter()
            .ok_or(Error::InvalidData)?;

        let node = self.node.read().unwrap();
        for attr_path_ib in attr_list {
            let attr_path = attr_path::Ib::from_tlv(&attr_path_ib)?;
            let result = node.for_each_attribute(&attr_path.path, |path, c| {
                let attr_id = if let Some(a) = path.leaf { a } else { 0 } as u16;
                let attr_path = attr_path::Ib::new(path);
                let attr_value =
                    |tag: TagType, tw: &mut TLVWriter| c.read_attribute(tag, tw, attr_id);
                // For now, putting everything in here
                let attr_resp = attr_response::Ib::AttrData(attr_path, attr_value);
                let _ = tw.put_object(TagType::Anonymous, &attr_resp);
                Ok(())
            });
            if let Err(e) = result {
                let attr_resp =
                    attr_response::Ib::AttrStatus(attr_path, e, 0, attr_response::dummy);
                let _ = tw.put_object(TagType::Anonymous, &attr_resp);
            }
        }
        Ok(())
    }

    fn consume_invoke_cmd(
        &self,
        cmd_path_ib: &CmdPathIb,
        data: TLVElement,
        trans: &mut Transaction,
        tlvwriter: &mut TLVWriter,
    ) -> Result<(), Error> {
        info!("Invoke Commmand Handler executing: {:?}", cmd_path_ib);

        let cmd_req = CommandReq {
            // TODO: Need to support wildcards
            endpoint: cmd_path_ib.endpoint.unwrap_or(1),
            cluster: cmd_path_ib.cluster.unwrap_or(0),
            command: cmd_path_ib.command,
            data,
            trans,
            resp: tlvwriter,
        };
        let cmd_path_ib = cmd_req.to_cmd_path_ib();

        let result = self.handle_command(cmd_req);
        if let Err(result) = result {
            // Err return implies we must send the StatusIB with this code
            let invoke_resp = InvokeRespIb::CommandStatus(cmd_path_ib, result, 0, command::dummy);
            tlvwriter.put_object(TagType::Anonymous, &invoke_resp)?;
        }
        Ok(())
    }
}
