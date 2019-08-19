use rand::distributions::Distribution;
use rand::distributions::Standard;
use rand::Rng;
use shared_memory::EventState;
use shared_memory::EventSet;
use shared_memory::EventType;
use shared_memory::EventWait;
use shared_memory::SharedMem;
use shared_memory::SharedMemConf;
use shared_memory::Timeout;
use std::error::Error;
use std::sync::atomic::AtomicIsize;
use std::sync::atomic::Ordering;
use std::time::Instant;

#[cfg(feature="ipc")]
use ipc_channel::ipc;
#[cfg(feature="ipc")]
use serde::{de, ser, Serialize, Deserialize};

#[cfg_attr(feature = "ipc", derive(Serialize, Deserialize))]
enum Foo {
    A(Bar),
    B(u32),
}

#[cfg_attr(feature = "ipc", derive(Serialize, Deserialize))]
enum Bar {
    A(f64),
    B(u32),
}

impl Distribution<Foo> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Foo {
        if rng.gen() {
	    Foo::A(rng.gen())
	} else {
	    Foo::B(rng.gen())
	}
    }
}

impl Distribution<Bar> for Standard {
    fn sample<R: Rng + ?Sized>(&self, rng: &mut R) -> Bar {
        if rng.gen() {
	    Bar::A(rng.gen())
	} else {
	    Bar::B(rng.gen())
	}
    }
}

struct Sender<T> {
    shmem: SharedMem,
    base: *mut Option<T>,
    finish: *mut AtomicIsize,
    size: *mut AtomicIsize,
    condvar: usize,
    capacity: isize,
}

impl<T> Sender<T> {
    unsafe fn from_shmem(shmem: SharedMem) -> Self {
        let size = shmem.get_ptr() as *mut AtomicIsize;
        let finish = size.offset(1);
	let base = size.offset(2) as *mut Option<T>;
	let capacity = ((shmem.get_size() - 16) / std::mem::size_of::<Option<T>>()) as isize;
	let condvar = 0;
        Sender {
	    shmem,
	    size,
	    finish,
	    base,
	    capacity,
	    condvar,
	}
    }
    fn send(&mut self, data: T) {
        let size = unsafe { &*self.size }.fetch_add(1, Ordering::SeqCst);
	if size >= self.capacity {
	   // The buffer is full, give up
	   unsafe { &*self.size }.fetch_sub(1, Ordering::SeqCst);
	   return;
	}
	let index = unsafe { &*self.finish }.fetch_add(1, Ordering::SeqCst) % self.capacity;
	if index == 0 {
	   // We overflowed, but the buffer is circular, so we just mod
	   unsafe { &*self.finish }.fetch_sub(self.capacity, Ordering::SeqCst);
	}
	unsafe { self.base.offset(index).write(Some(data)); }
	self.shmem.set(self.condvar, EventState::Signaled);
    }
}

struct Receiver<T> {
    shmem: SharedMem,
    base: *mut Option<T>,
    size: *mut AtomicIsize,
    condvar: usize,
    capacity: isize,
    start: isize,
}

impl<T> Receiver<T> {
    unsafe fn from_shmem(shmem: SharedMem) -> Self {
        let size = shmem.get_ptr() as *mut AtomicIsize;
	let start = 1;
        let finish = size.offset(1);
	let base = size.offset(2) as *mut Option<T>;
	let capacity = ((shmem.get_size() - 16) / std::mem::size_of::<Option<T>>()) as isize;
	let condvar = 0;
	(&*size).store(0, Ordering::SeqCst);
	(&*finish).store(start, Ordering::SeqCst);
	for i in 0..capacity {
	    base.offset(i).write(None);
	}
        Receiver {
	    shmem,
	    size,
	    start,
	    base,
	    capacity,
	    condvar,
	}
    }
    fn try_recv(&mut self) -> Option<T> {
        let result = unsafe { &mut*self.base.offset(self.start) }.take();
        if !result.is_none() {
	    self.start = (self.start + 1) % self.capacity;
	    unsafe { &*self.size }.fetch_sub(1, Ordering::SeqCst);
	}
        result
    }
    fn recv(&mut self) -> T {
        loop {
	    match self.try_recv() {
	        None => { let _ = self.shmem.wait(self.condvar, Timeout::Infinite); },
	        Some(result) => return result,
   	    }
	}
    }
    fn try_peek(&self) -> Option<&T> {
        unsafe { &mut*self.base.offset(self.start) }.as_ref()
    }
    fn peek(&mut self) -> &T {
        loop {
	    match unsafe { &mut*self.base.offset(self.start) } {
	        None => { let _ = self.shmem.wait(self.condvar, Timeout::Infinite); },
	        Some(ref result) => return result,
   	    }
	}
    }
}

const ITERATIONS: usize = 1_000_000;

fn server() {
    #[cfg(not(feature = "ipc"))]
    let mut receiver = {
        let shmem = SharedMemConf::new()
            .set_size(1024 * 1024)
	    .add_event(EventType::Auto).unwrap()
	    .create().unwrap();
        println!("Created shmem at {}", shmem.get_os_path());
        let mut receiver = unsafe { Receiver::from_shmem(shmem) };
        receiver.peek();
	receiver
    };
    #[cfg(feature = "ipc")]
    let receiver: ipc::IpcReceiver<Foo> = {
        let (server, name) = ipc::IpcOneShotServer::new().unwrap();
        println!("Created ipc at {}", name);
	let (_, receiver) = server.accept().unwrap();
        receiver
    };
    let mut total = 0.0;
    let start = Instant::now();
    for _ in 0..ITERATIONS {
        #[cfg(not(feature = "ipc"))]
        let msg = receiver.peek();
        #[cfg(feature = "ipc")]
        let msg = receiver.recv().unwrap();
        if let Foo::A(Bar::A(x)) = msg {
	   total += x;
	}
        #[cfg(not(feature = "ipc"))]
        receiver.recv();
    }
    let elapsed = Instant::now() - start;
    println!("Took {:?}", elapsed);
    println!("Total = {}", total);
}

fn client(name: String) {
    #[cfg(not(feature = "ipc"))]
    let mut sender = {
        let shmem = SharedMem::open(&name).unwrap();
        println!("Using shmem at {}", shmem.get_os_path());
        let mut sender = unsafe { Sender::from_shmem(shmem) };
        sender.send(Foo::B(0));
	sender
    };
    #[cfg(feature = "ipc")]
    let sender: ipc::IpcSender<Foo> = {
        let server = ipc::IpcSender::connect(name).unwrap();
	let (sender, receiver) = ipc::channel().unwrap();
	let _ = server.send(receiver);
        sender
    };
    let mut total = 0.0;
    for _ in 0..ITERATIONS {
        let data = rand::random();
        if let Foo::A(Bar::A(x)) = data {
	   total += x;
	}
	let _ = sender.send(data);
    }
    println!("Total = {}", total);
}

fn main() {
    if let Some(arg) = std::env::args().skip(1).next() {
        client(arg)
    } else {
        server()
    }
}