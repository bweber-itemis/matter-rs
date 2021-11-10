use crate::error::*;
use std::fmt;

/* This file needs some major revamp.
 * - instead of allocating all over the heap, we should use some kind of slab/block allocator
 * - instead of arrays, can use linked-lists to conserve space and avoid the internal fragmentation
 */

pub const ENDPTS_PER_ACC: usize = 3;
pub const CLUSTERS_PER_ENDPT: usize = 4;
pub const ATTRS_PER_CLUSTER: usize = 4;
pub const CMDS_PER_CLUSTER: usize = 4;

#[derive(Debug)]
pub enum AttrValue {
    Int8(i8),
    Int64(i64),
    Uint16(u16),
    Bool(bool),
}

#[derive(Debug)]
pub struct Attribute {
    id: u32,
    value: AttrValue,
}

impl Default for Attribute {
    fn default() -> Attribute {
        Attribute {
            id: 0,
            value: AttrValue::Bool(true),
        }
    }
}

impl Attribute {
    pub fn new(id: u32, val: AttrValue) -> Result<Box<Attribute>, Error> {
        let mut a = Box::new(Attribute::default());
        a.id = id;
        a.value = val;
        Ok(a)
    }
}

impl std::fmt::Display for Attribute {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {:?}", self.id, self.value)
    }
}

pub type CommandCb = fn(&mut Cluster, id: u16) -> Result<(), Error>;

pub struct Command {
    id: u16,
    cb: CommandCb,
}

impl std::fmt::Display for Command {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "id:{}", self.id)
    }
}

impl Command {
    pub fn new(id: u16, cb: CommandCb) -> Result<Box<Command>, Error> {
        Ok(Box::new(Command { id, cb }))
    }
}

#[derive(Default)]
pub struct Cluster {
    id: u32,
    attributes: [Option<Box<Attribute>>; ATTRS_PER_CLUSTER],
    commands: [Option<Box<Command>>; CMDS_PER_CLUSTER],
}

impl Cluster {
    pub fn new(id: u32) -> Result<Box<Cluster>, Error> {
        let mut a = Box::new(Cluster::default());
        a.id = id;
        Ok(a)
    }

    pub fn add_attribute(&mut self, attr: Box<Attribute>) -> Result<(), Error> {
        for c in self.attributes.iter_mut() {
            if let None = c {
                *c = Some(attr);
                return Ok(());
            }
        }
        return Err(Error::NoSpace);
    }

    pub fn add_command(&mut self, command: Box<Command>) -> Result<(), Error> {
        for c in self.commands.iter_mut() {
            if let None = c {
                *c = Some(command);
                return Ok(());
            }
        }
        return Err(Error::NoSpace);
    }

    pub fn handle_command(&mut self, cmd_id: u16) -> Result<(), Error> {
        let cmd = self
            .commands
            .iter()
            .find(|x| x.as_ref().map_or(false, |c| c.id == cmd_id))
            .ok_or(Error::Invalid)?
            .as_ref()
            .ok_or(Error::Invalid)?;
        (cmd.cb)(self, cmd_id)
    }
}

impl std::fmt::Display for Cluster {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "id:{}, ", self.id)?;
        write!(f, "attrs[")?;
        let mut comma = "";
        for element in self.attributes.iter() {
            if let Some(e) = element {
                write!(f, "{} {}", comma, e)?;
            }
            comma = ",";
        }
        write!(f, " ], ")?;
        write!(f, "cmds[")?;
        let mut comma = "";
        for element in self.commands.iter() {
            if let Some(e) = element {
                write!(f, "{} {}", comma, e)?;
            }
            comma = ",";
        }
        write!(f, " ]")
    }
}

#[derive(Default)]
pub struct Endpoint {
    clusters: [Option<Box<Cluster>>; CLUSTERS_PER_ENDPT],
}

impl Endpoint {
    pub fn new() -> Result<Box<Endpoint>, Error> {
        Ok(Box::new(Endpoint::default()))
    }

    pub fn add_cluster(&mut self, cluster: Box<Cluster>) -> Result<(), Error> {
        for c in self.clusters.iter_mut() {
            if let None = c {
                *c = Some(cluster);
                return Ok(());
            }
        }
        return Err(Error::NoSpace);
    }

    pub fn get_cluster(&mut self, cluster_id: u32) -> Result<&mut Box<Cluster>, Error> {
        let index = self
            .clusters
            .iter()
            .position(|x| x.as_ref().map_or(false, |c| c.id == cluster_id))
            .ok_or(Error::Invalid)?;
        Ok(self.clusters[index].as_mut().unwrap())
    }
}

impl std::fmt::Display for Endpoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "clusters:[")?;
        let mut comma = "";
        for element in self.clusters.iter() {
            if let Some(e) = element {
                write!(f, "{} {{ {} }}", comma, e)?;
                comma = ", ";
            }
        }
        write!(f, "]")
    }
}

#[derive(Default)]
pub struct Node {
    endpoints: [Option<Box<Endpoint>>; ENDPTS_PER_ACC],
}

impl std::fmt::Display for Node {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "node:\n")?;
        for (i, element) in self.endpoints.iter().enumerate() {
            if let Some(e) = element {
                write!(f, "endpoint {}: {}\n", i, e)?;
            }
        }
        write!(f, "")
    }
}

impl Node {
    pub fn new() -> Result<Box<Node>, Error> {
        let node = Box::new(Node::default());
        Ok(node)
    }

    pub fn add_endpoint(&mut self) -> Result<u32, Error> {
        let index = self
            .endpoints
            .iter()
            .position(|x| x.is_none())
            .ok_or(Error::NoSpace)?;
        self.endpoints[index] = Some(Endpoint::new()?);
        Ok(index as u32)
    }

    pub fn get_endpoint(&mut self, endpoint_id: u32) -> Result<&mut Box<Endpoint>, Error> {
        let endpoint = self.endpoints[endpoint_id as usize]
            .as_mut()
            .ok_or(Error::Invalid)?;
        Ok(endpoint)
    }

    pub fn add_cluster(&mut self, endpoint_id: u32, cluster: Box<Cluster>) -> Result<(), Error> {
        let endpoint_id = endpoint_id as usize;

        self.endpoints[endpoint_id]
            .as_mut()
            .ok_or(Error::NoEndpoint)?
            .add_cluster(cluster)
    }
}
