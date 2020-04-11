pragma solidity ^0.6.3;
contract IO {
    
    bytes32[] store;
    
    constructor () public {
        uint64 counter = 0;
        while ( counter < 255 ) {
            store.push(hex"00");
            counter = counter + 1;
        }
    }

    
    function set(uint key, bytes32 value) public {
        store[key] = value;
    }
    
    function write(uint size) public {
        for (uint i = 0; i < size; i++) {
            store[i] = hex"00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";
        }
    }
    
    function scan(uint size) public view {
        bytes32 ret;
        for (uint i = 0; i < size; i++) {
            ret = store[i];
        }
    }
    
    function revert_scan(uint size) public view{
        bytes32 ret;
        for (uint i = 0; i < size; i++) {
            ret = store[size - i - 1];
        }
    }

    function run_all(uint size) public {
        write(size);
        scan(size);
        revert_scan(size);
    }

}

