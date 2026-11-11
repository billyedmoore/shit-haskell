{-# LANGUAGE MagicHash, UnboxedTuples, NoImplicitPrelude, PolyKinds, KindSignatures #-}
-- Generated from GHC's primops.txt.pp (GHC 9.12.2). Do not edit by hand.
module GHC.Prim where

data Char# 
data Int8# 
data Word8# 
data Int16# 
data Word16# 
data Int32# 
data Word32# 
data Int64# 
data Word64# 
data Int# 
data Word# 
data Double# 
data Float# 
data Array# a
data MutableArray# s a
data SmallArray# a
data SmallMutableArray# s a
data ByteArray# 
data MutableByteArray# s
data Addr# 
data MutVar# s a
data PromptTag# a
data TVar# s a
data MVar# s a
data IOPort# s a
data State# s
data RealWorld 
data ThreadId# 
data Weak# b
data StablePtr# a
data StableName# a
data Compact# 
data BCO 
data FUN m a b
data Proxy# a
data StackSnapshot# 
data VECTOR 
foreign import prim "realWorld#" realWorld# :: State# RealWorld
foreign import prim "nullAddr#" nullAddr# :: Addr#
foreign import prim "proxy#" proxy# :: Proxy# a
foreign import prim "seq" seq :: a -> b -> b
foreign import prim "unsafeCoerce" coerce :: a -> b
foreign import prim "indexCharArray#" indexCharArray# :: ByteArray# -> Int# -> Char#
foreign import prim "readCharArray#" readCharArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeCharArray#" writeCharArray# :: MutableByteArray# s -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexCharOffAddr#" indexCharOffAddr# :: Addr# -> Int# -> Char#
foreign import prim "readCharOffAddr#" readCharOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeCharOffAddr#" writeCharOffAddr# :: Addr# -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexWideCharArray#" indexWideCharArray# :: ByteArray# -> Int# -> Char#
foreign import prim "readWideCharArray#" readWideCharArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeWideCharArray#" writeWideCharArray# :: MutableByteArray# s -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexWideCharOffAddr#" indexWideCharOffAddr# :: Addr# -> Int# -> Char#
foreign import prim "readWideCharOffAddr#" readWideCharOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeWideCharOffAddr#" writeWideCharOffAddr# :: Addr# -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexIntArray#" indexIntArray# :: ByteArray# -> Int# -> Int#
foreign import prim "readIntArray#" readIntArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "writeIntArray#" writeIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> State# s
foreign import prim "indexIntOffAddr#" indexIntOffAddr# :: Addr# -> Int# -> Int#
foreign import prim "readIntOffAddr#" readIntOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "writeIntOffAddr#" writeIntOffAddr# :: Addr# -> Int# -> Int# -> State# s -> State# s
foreign import prim "indexWordArray#" indexWordArray# :: ByteArray# -> Int# -> Word#
foreign import prim "readWordArray#" readWordArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word# #)
foreign import prim "writeWordArray#" writeWordArray# :: MutableByteArray# s -> Int# -> Word# -> State# s -> State# s
foreign import prim "indexWordOffAddr#" indexWordOffAddr# :: Addr# -> Int# -> Word#
foreign import prim "readWordOffAddr#" readWordOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Word# #)
foreign import prim "writeWordOffAddr#" writeWordOffAddr# :: Addr# -> Int# -> Word# -> State# s -> State# s
foreign import prim "indexInt8Array#" indexInt8Array# :: ByteArray# -> Int# -> Int8#
foreign import prim "readInt8Array#" readInt8Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int8# #)
foreign import prim "writeInt8Array#" writeInt8Array# :: MutableByteArray# s -> Int# -> Int8# -> State# s -> State# s
foreign import prim "indexInt8OffAddr#" indexInt8OffAddr# :: Addr# -> Int# -> Int8#
foreign import prim "readInt8OffAddr#" readInt8OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Int8# #)
foreign import prim "writeInt8OffAddr#" writeInt8OffAddr# :: Addr# -> Int# -> Int8# -> State# s -> State# s
foreign import prim "indexInt16Array#" indexInt16Array# :: ByteArray# -> Int# -> Int16#
foreign import prim "readInt16Array#" readInt16Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int16# #)
foreign import prim "writeInt16Array#" writeInt16Array# :: MutableByteArray# s -> Int# -> Int16# -> State# s -> State# s
foreign import prim "indexInt16OffAddr#" indexInt16OffAddr# :: Addr# -> Int# -> Int16#
foreign import prim "readInt16OffAddr#" readInt16OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Int16# #)
foreign import prim "writeInt16OffAddr#" writeInt16OffAddr# :: Addr# -> Int# -> Int16# -> State# s -> State# s
foreign import prim "indexInt32Array#" indexInt32Array# :: ByteArray# -> Int# -> Int32#
foreign import prim "readInt32Array#" readInt32Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int32# #)
foreign import prim "writeInt32Array#" writeInt32Array# :: MutableByteArray# s -> Int# -> Int32# -> State# s -> State# s
foreign import prim "indexInt32OffAddr#" indexInt32OffAddr# :: Addr# -> Int# -> Int32#
foreign import prim "readInt32OffAddr#" readInt32OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Int32# #)
foreign import prim "writeInt32OffAddr#" writeInt32OffAddr# :: Addr# -> Int# -> Int32# -> State# s -> State# s
foreign import prim "indexInt64Array#" indexInt64Array# :: ByteArray# -> Int# -> Int64#
foreign import prim "readInt64Array#" readInt64Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int64# #)
foreign import prim "writeInt64Array#" writeInt64Array# :: MutableByteArray# s -> Int# -> Int64# -> State# s -> State# s
foreign import prim "indexInt64OffAddr#" indexInt64OffAddr# :: Addr# -> Int# -> Int64#
foreign import prim "readInt64OffAddr#" readInt64OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Int64# #)
foreign import prim "writeInt64OffAddr#" writeInt64OffAddr# :: Addr# -> Int# -> Int64# -> State# s -> State# s
foreign import prim "indexWord8Array#" indexWord8Array# :: ByteArray# -> Int# -> Word8#
foreign import prim "readWord8Array#" readWord8Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word8# #)
foreign import prim "writeWord8Array#" writeWord8Array# :: MutableByteArray# s -> Int# -> Word8# -> State# s -> State# s
foreign import prim "indexWord8OffAddr#" indexWord8OffAddr# :: Addr# -> Int# -> Word8#
foreign import prim "readWord8OffAddr#" readWord8OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Word8# #)
foreign import prim "writeWord8OffAddr#" writeWord8OffAddr# :: Addr# -> Int# -> Word8# -> State# s -> State# s
foreign import prim "indexWord16Array#" indexWord16Array# :: ByteArray# -> Int# -> Word16#
foreign import prim "readWord16Array#" readWord16Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word16# #)
foreign import prim "writeWord16Array#" writeWord16Array# :: MutableByteArray# s -> Int# -> Word16# -> State# s -> State# s
foreign import prim "indexWord16OffAddr#" indexWord16OffAddr# :: Addr# -> Int# -> Word16#
foreign import prim "readWord16OffAddr#" readWord16OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Word16# #)
foreign import prim "writeWord16OffAddr#" writeWord16OffAddr# :: Addr# -> Int# -> Word16# -> State# s -> State# s
foreign import prim "indexWord32Array#" indexWord32Array# :: ByteArray# -> Int# -> Word32#
foreign import prim "readWord32Array#" readWord32Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word32# #)
foreign import prim "writeWord32Array#" writeWord32Array# :: MutableByteArray# s -> Int# -> Word32# -> State# s -> State# s
foreign import prim "indexWord32OffAddr#" indexWord32OffAddr# :: Addr# -> Int# -> Word32#
foreign import prim "readWord32OffAddr#" readWord32OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Word32# #)
foreign import prim "writeWord32OffAddr#" writeWord32OffAddr# :: Addr# -> Int# -> Word32# -> State# s -> State# s
foreign import prim "indexWord64Array#" indexWord64Array# :: ByteArray# -> Int# -> Word64#
foreign import prim "readWord64Array#" readWord64Array# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word64# #)
foreign import prim "writeWord64Array#" writeWord64Array# :: MutableByteArray# s -> Int# -> Word64# -> State# s -> State# s
foreign import prim "indexWord64OffAddr#" indexWord64OffAddr# :: Addr# -> Int# -> Word64#
foreign import prim "readWord64OffAddr#" readWord64OffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Word64# #)
foreign import prim "writeWord64OffAddr#" writeWord64OffAddr# :: Addr# -> Int# -> Word64# -> State# s -> State# s
foreign import prim "indexDoubleArray#" indexDoubleArray# :: ByteArray# -> Int# -> Double#
foreign import prim "readDoubleArray#" readDoubleArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Double# #)
foreign import prim "writeDoubleArray#" writeDoubleArray# :: MutableByteArray# s -> Int# -> Double# -> State# s -> State# s
foreign import prim "indexDoubleOffAddr#" indexDoubleOffAddr# :: Addr# -> Int# -> Double#
foreign import prim "readDoubleOffAddr#" readDoubleOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Double# #)
foreign import prim "writeDoubleOffAddr#" writeDoubleOffAddr# :: Addr# -> Int# -> Double# -> State# s -> State# s
foreign import prim "indexFloatArray#" indexFloatArray# :: ByteArray# -> Int# -> Float#
foreign import prim "readFloatArray#" readFloatArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Float# #)
foreign import prim "writeFloatArray#" writeFloatArray# :: MutableByteArray# s -> Int# -> Float# -> State# s -> State# s
foreign import prim "indexFloatOffAddr#" indexFloatOffAddr# :: Addr# -> Int# -> Float#
foreign import prim "readFloatOffAddr#" readFloatOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Float# #)
foreign import prim "writeFloatOffAddr#" writeFloatOffAddr# :: Addr# -> Int# -> Float# -> State# s -> State# s
foreign import prim "indexAddrArray#" indexAddrArray# :: ByteArray# -> Int# -> Addr#
foreign import prim "readAddrArray#" readAddrArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Addr# #)
foreign import prim "writeAddrArray#" writeAddrArray# :: MutableByteArray# s -> Int# -> Addr# -> State# s -> State# s
foreign import prim "indexAddrOffAddr#" indexAddrOffAddr# :: Addr# -> Int# -> Addr#
foreign import prim "readAddrOffAddr#" readAddrOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, Addr# #)
foreign import prim "writeAddrOffAddr#" writeAddrOffAddr# :: Addr# -> Int# -> Addr# -> State# s -> State# s
foreign import prim "indexStablePtrArray#" indexStablePtrArray# :: ByteArray# -> Int# -> StablePtr# a
foreign import prim "readStablePtrArray#" readStablePtrArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, StablePtr# a #)
foreign import prim "writeStablePtrArray#" writeStablePtrArray# :: MutableByteArray# s -> Int# -> StablePtr# a -> State# s -> State# s
foreign import prim "indexStablePtrOffAddr#" indexStablePtrOffAddr# :: Addr# -> Int# -> StablePtr# a
foreign import prim "readStablePtrOffAddr#" readStablePtrOffAddr# :: Addr# -> Int# -> State# s -> (# State# s, StablePtr# a #)
foreign import prim "writeStablePtrOffAddr#" writeStablePtrOffAddr# :: Addr# -> Int# -> StablePtr# a -> State# s -> State# s
foreign import prim "indexWord8ArrayAsChar#" indexWord8ArrayAsChar# :: ByteArray# -> Int# -> Char#
foreign import prim "readWord8ArrayAsChar#" readWord8ArrayAsChar# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeWord8ArrayAsChar#" writeWord8ArrayAsChar# :: MutableByteArray# s -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsChar#" indexWord8OffAddrAsChar# :: Addr# -> Int# -> Char#
foreign import prim "readWord8OffAddrAsChar#" readWord8OffAddrAsChar# :: Addr# -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeWord8OffAddrAsChar#" writeWord8OffAddrAsChar# :: Addr# -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsWideChar#" indexWord8ArrayAsWideChar# :: ByteArray# -> Int# -> Char#
foreign import prim "readWord8ArrayAsWideChar#" readWord8ArrayAsWideChar# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeWord8ArrayAsWideChar#" writeWord8ArrayAsWideChar# :: MutableByteArray# s -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsWideChar#" indexWord8OffAddrAsWideChar# :: Addr# -> Int# -> Char#
foreign import prim "readWord8OffAddrAsWideChar#" readWord8OffAddrAsWideChar# :: Addr# -> Int# -> State# s -> (# State# s, Char# #)
foreign import prim "writeWord8OffAddrAsWideChar#" writeWord8OffAddrAsWideChar# :: Addr# -> Int# -> Char# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsInt#" indexWord8ArrayAsInt# :: ByteArray# -> Int# -> Int#
foreign import prim "readWord8ArrayAsInt#" readWord8ArrayAsInt# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "writeWord8ArrayAsInt#" writeWord8ArrayAsInt# :: MutableByteArray# s -> Int# -> Int# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsInt#" indexWord8OffAddrAsInt# :: Addr# -> Int# -> Int#
foreign import prim "readWord8OffAddrAsInt#" readWord8OffAddrAsInt# :: Addr# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "writeWord8OffAddrAsInt#" writeWord8OffAddrAsInt# :: Addr# -> Int# -> Int# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsWord#" indexWord8ArrayAsWord# :: ByteArray# -> Int# -> Word#
foreign import prim "readWord8ArrayAsWord#" readWord8ArrayAsWord# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word# #)
foreign import prim "writeWord8ArrayAsWord#" writeWord8ArrayAsWord# :: MutableByteArray# s -> Int# -> Word# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsWord#" indexWord8OffAddrAsWord# :: Addr# -> Int# -> Word#
foreign import prim "readWord8OffAddrAsWord#" readWord8OffAddrAsWord# :: Addr# -> Int# -> State# s -> (# State# s, Word# #)
foreign import prim "writeWord8OffAddrAsWord#" writeWord8OffAddrAsWord# :: Addr# -> Int# -> Word# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsInt16#" indexWord8ArrayAsInt16# :: ByteArray# -> Int# -> Int16#
foreign import prim "readWord8ArrayAsInt16#" readWord8ArrayAsInt16# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int16# #)
foreign import prim "writeWord8ArrayAsInt16#" writeWord8ArrayAsInt16# :: MutableByteArray# s -> Int# -> Int16# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsInt16#" indexWord8OffAddrAsInt16# :: Addr# -> Int# -> Int16#
foreign import prim "readWord8OffAddrAsInt16#" readWord8OffAddrAsInt16# :: Addr# -> Int# -> State# s -> (# State# s, Int16# #)
foreign import prim "writeWord8OffAddrAsInt16#" writeWord8OffAddrAsInt16# :: Addr# -> Int# -> Int16# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsInt32#" indexWord8ArrayAsInt32# :: ByteArray# -> Int# -> Int32#
foreign import prim "readWord8ArrayAsInt32#" readWord8ArrayAsInt32# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int32# #)
foreign import prim "writeWord8ArrayAsInt32#" writeWord8ArrayAsInt32# :: MutableByteArray# s -> Int# -> Int32# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsInt32#" indexWord8OffAddrAsInt32# :: Addr# -> Int# -> Int32#
foreign import prim "readWord8OffAddrAsInt32#" readWord8OffAddrAsInt32# :: Addr# -> Int# -> State# s -> (# State# s, Int32# #)
foreign import prim "writeWord8OffAddrAsInt32#" writeWord8OffAddrAsInt32# :: Addr# -> Int# -> Int32# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsInt64#" indexWord8ArrayAsInt64# :: ByteArray# -> Int# -> Int64#
foreign import prim "readWord8ArrayAsInt64#" readWord8ArrayAsInt64# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int64# #)
foreign import prim "writeWord8ArrayAsInt64#" writeWord8ArrayAsInt64# :: MutableByteArray# s -> Int# -> Int64# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsInt64#" indexWord8OffAddrAsInt64# :: Addr# -> Int# -> Int64#
foreign import prim "readWord8OffAddrAsInt64#" readWord8OffAddrAsInt64# :: Addr# -> Int# -> State# s -> (# State# s, Int64# #)
foreign import prim "writeWord8OffAddrAsInt64#" writeWord8OffAddrAsInt64# :: Addr# -> Int# -> Int64# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsWord16#" indexWord8ArrayAsWord16# :: ByteArray# -> Int# -> Word16#
foreign import prim "readWord8ArrayAsWord16#" readWord8ArrayAsWord16# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word16# #)
foreign import prim "writeWord8ArrayAsWord16#" writeWord8ArrayAsWord16# :: MutableByteArray# s -> Int# -> Word16# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsWord16#" indexWord8OffAddrAsWord16# :: Addr# -> Int# -> Word16#
foreign import prim "readWord8OffAddrAsWord16#" readWord8OffAddrAsWord16# :: Addr# -> Int# -> State# s -> (# State# s, Word16# #)
foreign import prim "writeWord8OffAddrAsWord16#" writeWord8OffAddrAsWord16# :: Addr# -> Int# -> Word16# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsWord32#" indexWord8ArrayAsWord32# :: ByteArray# -> Int# -> Word32#
foreign import prim "readWord8ArrayAsWord32#" readWord8ArrayAsWord32# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word32# #)
foreign import prim "writeWord8ArrayAsWord32#" writeWord8ArrayAsWord32# :: MutableByteArray# s -> Int# -> Word32# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsWord32#" indexWord8OffAddrAsWord32# :: Addr# -> Int# -> Word32#
foreign import prim "readWord8OffAddrAsWord32#" readWord8OffAddrAsWord32# :: Addr# -> Int# -> State# s -> (# State# s, Word32# #)
foreign import prim "writeWord8OffAddrAsWord32#" writeWord8OffAddrAsWord32# :: Addr# -> Int# -> Word32# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsWord64#" indexWord8ArrayAsWord64# :: ByteArray# -> Int# -> Word64#
foreign import prim "readWord8ArrayAsWord64#" readWord8ArrayAsWord64# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Word64# #)
foreign import prim "writeWord8ArrayAsWord64#" writeWord8ArrayAsWord64# :: MutableByteArray# s -> Int# -> Word64# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsWord64#" indexWord8OffAddrAsWord64# :: Addr# -> Int# -> Word64#
foreign import prim "readWord8OffAddrAsWord64#" readWord8OffAddrAsWord64# :: Addr# -> Int# -> State# s -> (# State# s, Word64# #)
foreign import prim "writeWord8OffAddrAsWord64#" writeWord8OffAddrAsWord64# :: Addr# -> Int# -> Word64# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsDouble#" indexWord8ArrayAsDouble# :: ByteArray# -> Int# -> Double#
foreign import prim "readWord8ArrayAsDouble#" readWord8ArrayAsDouble# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Double# #)
foreign import prim "writeWord8ArrayAsDouble#" writeWord8ArrayAsDouble# :: MutableByteArray# s -> Int# -> Double# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsDouble#" indexWord8OffAddrAsDouble# :: Addr# -> Int# -> Double#
foreign import prim "readWord8OffAddrAsDouble#" readWord8OffAddrAsDouble# :: Addr# -> Int# -> State# s -> (# State# s, Double# #)
foreign import prim "writeWord8OffAddrAsDouble#" writeWord8OffAddrAsDouble# :: Addr# -> Int# -> Double# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsFloat#" indexWord8ArrayAsFloat# :: ByteArray# -> Int# -> Float#
foreign import prim "readWord8ArrayAsFloat#" readWord8ArrayAsFloat# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Float# #)
foreign import prim "writeWord8ArrayAsFloat#" writeWord8ArrayAsFloat# :: MutableByteArray# s -> Int# -> Float# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsFloat#" indexWord8OffAddrAsFloat# :: Addr# -> Int# -> Float#
foreign import prim "readWord8OffAddrAsFloat#" readWord8OffAddrAsFloat# :: Addr# -> Int# -> State# s -> (# State# s, Float# #)
foreign import prim "writeWord8OffAddrAsFloat#" writeWord8OffAddrAsFloat# :: Addr# -> Int# -> Float# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsAddr#" indexWord8ArrayAsAddr# :: ByteArray# -> Int# -> Addr#
foreign import prim "readWord8ArrayAsAddr#" readWord8ArrayAsAddr# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Addr# #)
foreign import prim "writeWord8ArrayAsAddr#" writeWord8ArrayAsAddr# :: MutableByteArray# s -> Int# -> Addr# -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsAddr#" indexWord8OffAddrAsAddr# :: Addr# -> Int# -> Addr#
foreign import prim "readWord8OffAddrAsAddr#" readWord8OffAddrAsAddr# :: Addr# -> Int# -> State# s -> (# State# s, Addr# #)
foreign import prim "writeWord8OffAddrAsAddr#" writeWord8OffAddrAsAddr# :: Addr# -> Int# -> Addr# -> State# s -> State# s
foreign import prim "indexWord8ArrayAsStablePtr#" indexWord8ArrayAsStablePtr# :: ByteArray# -> Int# -> StablePtr# a
foreign import prim "readWord8ArrayAsStablePtr#" readWord8ArrayAsStablePtr# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, StablePtr# a #)
foreign import prim "writeWord8ArrayAsStablePtr#" writeWord8ArrayAsStablePtr# :: MutableByteArray# s -> Int# -> StablePtr# a -> State# s -> State# s
foreign import prim "indexWord8OffAddrAsStablePtr#" indexWord8OffAddrAsStablePtr# :: Addr# -> Int# -> StablePtr# a
foreign import prim "readWord8OffAddrAsStablePtr#" readWord8OffAddrAsStablePtr# :: Addr# -> Int# -> State# s -> (# State# s, StablePtr# a #)
foreign import prim "writeWord8OffAddrAsStablePtr#" writeWord8OffAddrAsStablePtr# :: Addr# -> Int# -> StablePtr# a -> State# s -> State# s
data TYPE r
data CONSTRAINT r
foreign import prim "mkApUpd0#" mkApUpd0# :: BCO -> (# a #)

foreign import prim "gtChar#" gtChar# :: Char# -> Char# -> Int#
foreign import prim "geChar#" geChar# :: Char# -> Char# -> Int#
foreign import prim "eqChar#" eqChar# :: Char# -> Char# -> Int#
foreign import prim "neChar#" neChar# :: Char# -> Char# -> Int#
foreign import prim "ltChar#" ltChar# :: Char# -> Char# -> Int#
foreign import prim "leChar#" leChar# :: Char# -> Char# -> Int#
foreign import prim "ord#" ord# :: Char# -> Int#
foreign import prim "int8ToInt#" int8ToInt# :: Int8# -> Int#
foreign import prim "intToInt8#" intToInt8# :: Int# -> Int8#
foreign import prim "negateInt8#" negateInt8# :: Int8# -> Int8#
foreign import prim "plusInt8#" plusInt8# :: Int8# -> Int8# -> Int8#
foreign import prim "subInt8#" subInt8# :: Int8# -> Int8# -> Int8#
foreign import prim "timesInt8#" timesInt8# :: Int8# -> Int8# -> Int8#
foreign import prim "quotInt8#" quotInt8# :: Int8# -> Int8# -> Int8#
foreign import prim "remInt8#" remInt8# :: Int8# -> Int8# -> Int8#
foreign import prim "quotRemInt8#" quotRemInt8# :: Int8# -> Int8# -> (# Int8#, Int8# #)
foreign import prim "uncheckedShiftLInt8#" uncheckedShiftLInt8# :: Int8# -> Int# -> Int8#
foreign import prim "uncheckedShiftRAInt8#" uncheckedShiftRAInt8# :: Int8# -> Int# -> Int8#
foreign import prim "uncheckedShiftRLInt8#" uncheckedShiftRLInt8# :: Int8# -> Int# -> Int8#
foreign import prim "int8ToWord8#" int8ToWord8# :: Int8# -> Word8#
foreign import prim "eqInt8#" eqInt8# :: Int8# -> Int8# -> Int#
foreign import prim "geInt8#" geInt8# :: Int8# -> Int8# -> Int#
foreign import prim "gtInt8#" gtInt8# :: Int8# -> Int8# -> Int#
foreign import prim "leInt8#" leInt8# :: Int8# -> Int8# -> Int#
foreign import prim "ltInt8#" ltInt8# :: Int8# -> Int8# -> Int#
foreign import prim "neInt8#" neInt8# :: Int8# -> Int8# -> Int#
foreign import prim "word8ToWord#" word8ToWord# :: Word8# -> Word#
foreign import prim "wordToWord8#" wordToWord8# :: Word# -> Word8#
foreign import prim "plusWord8#" plusWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "subWord8#" subWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "timesWord8#" timesWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "quotWord8#" quotWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "remWord8#" remWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "quotRemWord8#" quotRemWord8# :: Word8# -> Word8# -> (# Word8#, Word8# #)
foreign import prim "andWord8#" andWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "orWord8#" orWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "xorWord8#" xorWord8# :: Word8# -> Word8# -> Word8#
foreign import prim "notWord8#" notWord8# :: Word8# -> Word8#
foreign import prim "uncheckedShiftLWord8#" uncheckedShiftLWord8# :: Word8# -> Int# -> Word8#
foreign import prim "uncheckedShiftRLWord8#" uncheckedShiftRLWord8# :: Word8# -> Int# -> Word8#
foreign import prim "word8ToInt8#" word8ToInt8# :: Word8# -> Int8#
foreign import prim "eqWord8#" eqWord8# :: Word8# -> Word8# -> Int#
foreign import prim "geWord8#" geWord8# :: Word8# -> Word8# -> Int#
foreign import prim "gtWord8#" gtWord8# :: Word8# -> Word8# -> Int#
foreign import prim "leWord8#" leWord8# :: Word8# -> Word8# -> Int#
foreign import prim "ltWord8#" ltWord8# :: Word8# -> Word8# -> Int#
foreign import prim "neWord8#" neWord8# :: Word8# -> Word8# -> Int#
foreign import prim "int16ToInt#" int16ToInt# :: Int16# -> Int#
foreign import prim "intToInt16#" intToInt16# :: Int# -> Int16#
foreign import prim "negateInt16#" negateInt16# :: Int16# -> Int16#
foreign import prim "plusInt16#" plusInt16# :: Int16# -> Int16# -> Int16#
foreign import prim "subInt16#" subInt16# :: Int16# -> Int16# -> Int16#
foreign import prim "timesInt16#" timesInt16# :: Int16# -> Int16# -> Int16#
foreign import prim "quotInt16#" quotInt16# :: Int16# -> Int16# -> Int16#
foreign import prim "remInt16#" remInt16# :: Int16# -> Int16# -> Int16#
foreign import prim "quotRemInt16#" quotRemInt16# :: Int16# -> Int16# -> (# Int16#, Int16# #)
foreign import prim "uncheckedShiftLInt16#" uncheckedShiftLInt16# :: Int16# -> Int# -> Int16#
foreign import prim "uncheckedShiftRAInt16#" uncheckedShiftRAInt16# :: Int16# -> Int# -> Int16#
foreign import prim "uncheckedShiftRLInt16#" uncheckedShiftRLInt16# :: Int16# -> Int# -> Int16#
foreign import prim "int16ToWord16#" int16ToWord16# :: Int16# -> Word16#
foreign import prim "eqInt16#" eqInt16# :: Int16# -> Int16# -> Int#
foreign import prim "geInt16#" geInt16# :: Int16# -> Int16# -> Int#
foreign import prim "gtInt16#" gtInt16# :: Int16# -> Int16# -> Int#
foreign import prim "leInt16#" leInt16# :: Int16# -> Int16# -> Int#
foreign import prim "ltInt16#" ltInt16# :: Int16# -> Int16# -> Int#
foreign import prim "neInt16#" neInt16# :: Int16# -> Int16# -> Int#
foreign import prim "word16ToWord#" word16ToWord# :: Word16# -> Word#
foreign import prim "wordToWord16#" wordToWord16# :: Word# -> Word16#
foreign import prim "plusWord16#" plusWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "subWord16#" subWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "timesWord16#" timesWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "quotWord16#" quotWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "remWord16#" remWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "quotRemWord16#" quotRemWord16# :: Word16# -> Word16# -> (# Word16#, Word16# #)
foreign import prim "andWord16#" andWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "orWord16#" orWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "xorWord16#" xorWord16# :: Word16# -> Word16# -> Word16#
foreign import prim "notWord16#" notWord16# :: Word16# -> Word16#
foreign import prim "uncheckedShiftLWord16#" uncheckedShiftLWord16# :: Word16# -> Int# -> Word16#
foreign import prim "uncheckedShiftRLWord16#" uncheckedShiftRLWord16# :: Word16# -> Int# -> Word16#
foreign import prim "word16ToInt16#" word16ToInt16# :: Word16# -> Int16#
foreign import prim "eqWord16#" eqWord16# :: Word16# -> Word16# -> Int#
foreign import prim "geWord16#" geWord16# :: Word16# -> Word16# -> Int#
foreign import prim "gtWord16#" gtWord16# :: Word16# -> Word16# -> Int#
foreign import prim "leWord16#" leWord16# :: Word16# -> Word16# -> Int#
foreign import prim "ltWord16#" ltWord16# :: Word16# -> Word16# -> Int#
foreign import prim "neWord16#" neWord16# :: Word16# -> Word16# -> Int#
foreign import prim "int32ToInt#" int32ToInt# :: Int32# -> Int#
foreign import prim "intToInt32#" intToInt32# :: Int# -> Int32#
foreign import prim "negateInt32#" negateInt32# :: Int32# -> Int32#
foreign import prim "plusInt32#" plusInt32# :: Int32# -> Int32# -> Int32#
foreign import prim "subInt32#" subInt32# :: Int32# -> Int32# -> Int32#
foreign import prim "timesInt32#" timesInt32# :: Int32# -> Int32# -> Int32#
foreign import prim "quotInt32#" quotInt32# :: Int32# -> Int32# -> Int32#
foreign import prim "remInt32#" remInt32# :: Int32# -> Int32# -> Int32#
foreign import prim "quotRemInt32#" quotRemInt32# :: Int32# -> Int32# -> (# Int32#, Int32# #)
foreign import prim "uncheckedShiftLInt32#" uncheckedShiftLInt32# :: Int32# -> Int# -> Int32#
foreign import prim "uncheckedShiftRAInt32#" uncheckedShiftRAInt32# :: Int32# -> Int# -> Int32#
foreign import prim "uncheckedShiftRLInt32#" uncheckedShiftRLInt32# :: Int32# -> Int# -> Int32#
foreign import prim "int32ToWord32#" int32ToWord32# :: Int32# -> Word32#
foreign import prim "eqInt32#" eqInt32# :: Int32# -> Int32# -> Int#
foreign import prim "geInt32#" geInt32# :: Int32# -> Int32# -> Int#
foreign import prim "gtInt32#" gtInt32# :: Int32# -> Int32# -> Int#
foreign import prim "leInt32#" leInt32# :: Int32# -> Int32# -> Int#
foreign import prim "ltInt32#" ltInt32# :: Int32# -> Int32# -> Int#
foreign import prim "neInt32#" neInt32# :: Int32# -> Int32# -> Int#
foreign import prim "word32ToWord#" word32ToWord# :: Word32# -> Word#
foreign import prim "wordToWord32#" wordToWord32# :: Word# -> Word32#
foreign import prim "plusWord32#" plusWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "subWord32#" subWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "timesWord32#" timesWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "quotWord32#" quotWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "remWord32#" remWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "quotRemWord32#" quotRemWord32# :: Word32# -> Word32# -> (# Word32#, Word32# #)
foreign import prim "andWord32#" andWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "orWord32#" orWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "xorWord32#" xorWord32# :: Word32# -> Word32# -> Word32#
foreign import prim "notWord32#" notWord32# :: Word32# -> Word32#
foreign import prim "uncheckedShiftLWord32#" uncheckedShiftLWord32# :: Word32# -> Int# -> Word32#
foreign import prim "uncheckedShiftRLWord32#" uncheckedShiftRLWord32# :: Word32# -> Int# -> Word32#
foreign import prim "word32ToInt32#" word32ToInt32# :: Word32# -> Int32#
foreign import prim "eqWord32#" eqWord32# :: Word32# -> Word32# -> Int#
foreign import prim "geWord32#" geWord32# :: Word32# -> Word32# -> Int#
foreign import prim "gtWord32#" gtWord32# :: Word32# -> Word32# -> Int#
foreign import prim "leWord32#" leWord32# :: Word32# -> Word32# -> Int#
foreign import prim "ltWord32#" ltWord32# :: Word32# -> Word32# -> Int#
foreign import prim "neWord32#" neWord32# :: Word32# -> Word32# -> Int#
foreign import prim "int64ToInt#" int64ToInt# :: Int64# -> Int#
foreign import prim "intToInt64#" intToInt64# :: Int# -> Int64#
foreign import prim "negateInt64#" negateInt64# :: Int64# -> Int64#
foreign import prim "plusInt64#" plusInt64# :: Int64# -> Int64# -> Int64#
foreign import prim "subInt64#" subInt64# :: Int64# -> Int64# -> Int64#
foreign import prim "timesInt64#" timesInt64# :: Int64# -> Int64# -> Int64#
foreign import prim "quotInt64#" quotInt64# :: Int64# -> Int64# -> Int64#
foreign import prim "remInt64#" remInt64# :: Int64# -> Int64# -> Int64#
foreign import prim "uncheckedIShiftL64#" uncheckedIShiftL64# :: Int64# -> Int# -> Int64#
foreign import prim "uncheckedIShiftRA64#" uncheckedIShiftRA64# :: Int64# -> Int# -> Int64#
foreign import prim "uncheckedIShiftRL64#" uncheckedIShiftRL64# :: Int64# -> Int# -> Int64#
foreign import prim "int64ToWord64#" int64ToWord64# :: Int64# -> Word64#
foreign import prim "eqInt64#" eqInt64# :: Int64# -> Int64# -> Int#
foreign import prim "geInt64#" geInt64# :: Int64# -> Int64# -> Int#
foreign import prim "gtInt64#" gtInt64# :: Int64# -> Int64# -> Int#
foreign import prim "leInt64#" leInt64# :: Int64# -> Int64# -> Int#
foreign import prim "ltInt64#" ltInt64# :: Int64# -> Int64# -> Int#
foreign import prim "neInt64#" neInt64# :: Int64# -> Int64# -> Int#
foreign import prim "word64ToWord#" word64ToWord# :: Word64# -> Word#
foreign import prim "wordToWord64#" wordToWord64# :: Word# -> Word64#
foreign import prim "plusWord64#" plusWord64# :: Word64# -> Word64# -> Word64#
foreign import prim "subWord64#" subWord64# :: Word64# -> Word64# -> Word64#
foreign import prim "timesWord64#" timesWord64# :: Word64# -> Word64# -> Word64#
foreign import prim "quotWord64#" quotWord64# :: Word64# -> Word64# -> Word64#
foreign import prim "remWord64#" remWord64# :: Word64# -> Word64# -> Word64#
foreign import prim "and64#" and64# :: Word64# -> Word64# -> Word64#
foreign import prim "or64#" or64# :: Word64# -> Word64# -> Word64#
foreign import prim "xor64#" xor64# :: Word64# -> Word64# -> Word64#
foreign import prim "not64#" not64# :: Word64# -> Word64#
foreign import prim "uncheckedShiftL64#" uncheckedShiftL64# :: Word64# -> Int# -> Word64#
foreign import prim "uncheckedShiftRL64#" uncheckedShiftRL64# :: Word64# -> Int# -> Word64#
foreign import prim "word64ToInt64#" word64ToInt64# :: Word64# -> Int64#
foreign import prim "eqWord64#" eqWord64# :: Word64# -> Word64# -> Int#
foreign import prim "geWord64#" geWord64# :: Word64# -> Word64# -> Int#
foreign import prim "gtWord64#" gtWord64# :: Word64# -> Word64# -> Int#
foreign import prim "leWord64#" leWord64# :: Word64# -> Word64# -> Int#
foreign import prim "ltWord64#" ltWord64# :: Word64# -> Word64# -> Int#
foreign import prim "neWord64#" neWord64# :: Word64# -> Word64# -> Int#
foreign import prim "+#" (+#) :: Int# -> Int# -> Int#
foreign import prim "-#" (-#) :: Int# -> Int# -> Int#
foreign import prim "*#" (*#) :: Int# -> Int# -> Int#
foreign import prim "timesInt2#" timesInt2# :: Int# -> Int# -> (# Int#, Int#, Int# #)
foreign import prim "mulIntMayOflo#" mulIntMayOflo# :: Int# -> Int# -> Int#
foreign import prim "quotInt#" quotInt# :: Int# -> Int# -> Int#
foreign import prim "remInt#" remInt# :: Int# -> Int# -> Int#
foreign import prim "quotRemInt#" quotRemInt# :: Int# -> Int# -> (# Int#, Int# #)
foreign import prim "andI#" andI# :: Int# -> Int# -> Int#
foreign import prim "orI#" orI# :: Int# -> Int# -> Int#
foreign import prim "xorI#" xorI# :: Int# -> Int# -> Int#
foreign import prim "notI#" notI# :: Int# -> Int#
foreign import prim "negateInt#" negateInt# :: Int# -> Int#
foreign import prim "addIntC#" addIntC# :: Int# -> Int# -> (# Int#, Int# #)
foreign import prim "subIntC#" subIntC# :: Int# -> Int# -> (# Int#, Int# #)
foreign import prim ">#" (>#) :: Int# -> Int# -> Int#
foreign import prim ">=#" (>=#) :: Int# -> Int# -> Int#
foreign import prim "==#" (==#) :: Int# -> Int# -> Int#
foreign import prim "/=#" (/=#) :: Int# -> Int# -> Int#
foreign import prim "<#" (<#) :: Int# -> Int# -> Int#
foreign import prim "<=#" (<=#) :: Int# -> Int# -> Int#
foreign import prim "chr#" chr# :: Int# -> Char#
foreign import prim "int2Word#" int2Word# :: Int# -> Word#
foreign import prim "int2Float#" int2Float# :: Int# -> Float#
foreign import prim "int2Double#" int2Double# :: Int# -> Double#
foreign import prim "word2Float#" word2Float# :: Word# -> Float#
foreign import prim "word2Double#" word2Double# :: Word# -> Double#
foreign import prim "uncheckedIShiftL#" uncheckedIShiftL# :: Int# -> Int# -> Int#
foreign import prim "uncheckedIShiftRA#" uncheckedIShiftRA# :: Int# -> Int# -> Int#
foreign import prim "uncheckedIShiftRL#" uncheckedIShiftRL# :: Int# -> Int# -> Int#
foreign import prim "plusWord#" plusWord# :: Word# -> Word# -> Word#
foreign import prim "addWordC#" addWordC# :: Word# -> Word# -> (# Word#, Int# #)
foreign import prim "subWordC#" subWordC# :: Word# -> Word# -> (# Word#, Int# #)
foreign import prim "plusWord2#" plusWord2# :: Word# -> Word# -> (# Word#, Word# #)
foreign import prim "minusWord#" minusWord# :: Word# -> Word# -> Word#
foreign import prim "timesWord#" timesWord# :: Word# -> Word# -> Word#
foreign import prim "timesWord2#" timesWord2# :: Word# -> Word# -> (# Word#, Word# #)
foreign import prim "quotWord#" quotWord# :: Word# -> Word# -> Word#
foreign import prim "remWord#" remWord# :: Word# -> Word# -> Word#
foreign import prim "quotRemWord#" quotRemWord# :: Word# -> Word# -> (# Word#, Word# #)
foreign import prim "quotRemWord2#" quotRemWord2# :: Word# -> Word# -> Word# -> (# Word#, Word# #)
foreign import prim "and#" and# :: Word# -> Word# -> Word#
foreign import prim "or#" or# :: Word# -> Word# -> Word#
foreign import prim "xor#" xor# :: Word# -> Word# -> Word#
foreign import prim "not#" not# :: Word# -> Word#
foreign import prim "uncheckedShiftL#" uncheckedShiftL# :: Word# -> Int# -> Word#
foreign import prim "uncheckedShiftRL#" uncheckedShiftRL# :: Word# -> Int# -> Word#
foreign import prim "word2Int#" word2Int# :: Word# -> Int#
foreign import prim "gtWord#" gtWord# :: Word# -> Word# -> Int#
foreign import prim "geWord#" geWord# :: Word# -> Word# -> Int#
foreign import prim "eqWord#" eqWord# :: Word# -> Word# -> Int#
foreign import prim "neWord#" neWord# :: Word# -> Word# -> Int#
foreign import prim "ltWord#" ltWord# :: Word# -> Word# -> Int#
foreign import prim "leWord#" leWord# :: Word# -> Word# -> Int#
foreign import prim "popCnt8#" popCnt8# :: Word# -> Word#
foreign import prim "popCnt16#" popCnt16# :: Word# -> Word#
foreign import prim "popCnt32#" popCnt32# :: Word# -> Word#
foreign import prim "popCnt64#" popCnt64# :: Word64# -> Word#
foreign import prim "popCnt#" popCnt# :: Word# -> Word#
foreign import prim "pdep8#" pdep8# :: Word# -> Word# -> Word#
foreign import prim "pdep16#" pdep16# :: Word# -> Word# -> Word#
foreign import prim "pdep32#" pdep32# :: Word# -> Word# -> Word#
foreign import prim "pdep64#" pdep64# :: Word64# -> Word64# -> Word64#
foreign import prim "pdep#" pdep# :: Word# -> Word# -> Word#
foreign import prim "pext8#" pext8# :: Word# -> Word# -> Word#
foreign import prim "pext16#" pext16# :: Word# -> Word# -> Word#
foreign import prim "pext32#" pext32# :: Word# -> Word# -> Word#
foreign import prim "pext64#" pext64# :: Word64# -> Word64# -> Word64#
foreign import prim "pext#" pext# :: Word# -> Word# -> Word#
foreign import prim "clz8#" clz8# :: Word# -> Word#
foreign import prim "clz16#" clz16# :: Word# -> Word#
foreign import prim "clz32#" clz32# :: Word# -> Word#
foreign import prim "clz64#" clz64# :: Word64# -> Word#
foreign import prim "clz#" clz# :: Word# -> Word#
foreign import prim "ctz8#" ctz8# :: Word# -> Word#
foreign import prim "ctz16#" ctz16# :: Word# -> Word#
foreign import prim "ctz32#" ctz32# :: Word# -> Word#
foreign import prim "ctz64#" ctz64# :: Word64# -> Word#
foreign import prim "ctz#" ctz# :: Word# -> Word#
foreign import prim "byteSwap16#" byteSwap16# :: Word# -> Word#
foreign import prim "byteSwap32#" byteSwap32# :: Word# -> Word#
foreign import prim "byteSwap64#" byteSwap64# :: Word64# -> Word64#
foreign import prim "byteSwap#" byteSwap# :: Word# -> Word#
foreign import prim "bitReverse8#" bitReverse8# :: Word# -> Word#
foreign import prim "bitReverse16#" bitReverse16# :: Word# -> Word#
foreign import prim "bitReverse32#" bitReverse32# :: Word# -> Word#
foreign import prim "bitReverse64#" bitReverse64# :: Word64# -> Word64#
foreign import prim "bitReverse#" bitReverse# :: Word# -> Word#
foreign import prim "narrow8Int#" narrow8Int# :: Int# -> Int#
foreign import prim "narrow16Int#" narrow16Int# :: Int# -> Int#
foreign import prim "narrow32Int#" narrow32Int# :: Int# -> Int#
foreign import prim "narrow8Word#" narrow8Word# :: Word# -> Word#
foreign import prim "narrow16Word#" narrow16Word# :: Word# -> Word#
foreign import prim "narrow32Word#" narrow32Word# :: Word# -> Word#
foreign import prim ">##" (>##) :: Double# -> Double# -> Int#
foreign import prim ">=##" (>=##) :: Double# -> Double# -> Int#
foreign import prim "==##" (==##) :: Double# -> Double# -> Int#
foreign import prim "/=##" (/=##) :: Double# -> Double# -> Int#
foreign import prim "<##" (<##) :: Double# -> Double# -> Int#
foreign import prim "<=##" (<=##) :: Double# -> Double# -> Int#
foreign import prim "minDouble#" minDouble# :: Double# -> Double# -> Double#
foreign import prim "maxDouble#" maxDouble# :: Double# -> Double# -> Double#
foreign import prim "+##" (+##) :: Double# -> Double# -> Double#
foreign import prim "-##" (-##) :: Double# -> Double# -> Double#
foreign import prim "*##" (*##) :: Double# -> Double# -> Double#
foreign import prim "/##" (/##) :: Double# -> Double# -> Double#
foreign import prim "negateDouble#" negateDouble# :: Double# -> Double#
foreign import prim "fabsDouble#" fabsDouble# :: Double# -> Double#
foreign import prim "double2Int#" double2Int# :: Double# -> Int#
foreign import prim "double2Float#" double2Float# :: Double# -> Float#
foreign import prim "expDouble#" expDouble# :: Double# -> Double#
foreign import prim "expm1Double#" expm1Double# :: Double# -> Double#
foreign import prim "logDouble#" logDouble# :: Double# -> Double#
foreign import prim "log1pDouble#" log1pDouble# :: Double# -> Double#
foreign import prim "sqrtDouble#" sqrtDouble# :: Double# -> Double#
foreign import prim "sinDouble#" sinDouble# :: Double# -> Double#
foreign import prim "cosDouble#" cosDouble# :: Double# -> Double#
foreign import prim "tanDouble#" tanDouble# :: Double# -> Double#
foreign import prim "asinDouble#" asinDouble# :: Double# -> Double#
foreign import prim "acosDouble#" acosDouble# :: Double# -> Double#
foreign import prim "atanDouble#" atanDouble# :: Double# -> Double#
foreign import prim "sinhDouble#" sinhDouble# :: Double# -> Double#
foreign import prim "coshDouble#" coshDouble# :: Double# -> Double#
foreign import prim "tanhDouble#" tanhDouble# :: Double# -> Double#
foreign import prim "asinhDouble#" asinhDouble# :: Double# -> Double#
foreign import prim "acoshDouble#" acoshDouble# :: Double# -> Double#
foreign import prim "atanhDouble#" atanhDouble# :: Double# -> Double#
foreign import prim "**##" (**##) :: Double# -> Double# -> Double#
foreign import prim "decodeDouble_2Int#" decodeDouble_2Int# :: Double# -> (# Int#, Word#, Word#, Int# #)
foreign import prim "decodeDouble_Int64#" decodeDouble_Int64# :: Double# -> (# Int64#, Int# #)
foreign import prim "castDoubleToWord64#" castDoubleToWord64# :: Double# -> Word64#
foreign import prim "castWord64ToDouble#" castWord64ToDouble# :: Word64# -> Double#
foreign import prim "gtFloat#" gtFloat# :: Float# -> Float# -> Int#
foreign import prim "geFloat#" geFloat# :: Float# -> Float# -> Int#
foreign import prim "eqFloat#" eqFloat# :: Float# -> Float# -> Int#
foreign import prim "neFloat#" neFloat# :: Float# -> Float# -> Int#
foreign import prim "ltFloat#" ltFloat# :: Float# -> Float# -> Int#
foreign import prim "leFloat#" leFloat# :: Float# -> Float# -> Int#
foreign import prim "minFloat#" minFloat# :: Float# -> Float# -> Float#
foreign import prim "maxFloat#" maxFloat# :: Float# -> Float# -> Float#
foreign import prim "plusFloat#" plusFloat# :: Float# -> Float# -> Float#
foreign import prim "minusFloat#" minusFloat# :: Float# -> Float# -> Float#
foreign import prim "timesFloat#" timesFloat# :: Float# -> Float# -> Float#
foreign import prim "divideFloat#" divideFloat# :: Float# -> Float# -> Float#
foreign import prim "negateFloat#" negateFloat# :: Float# -> Float#
foreign import prim "fabsFloat#" fabsFloat# :: Float# -> Float#
foreign import prim "float2Int#" float2Int# :: Float# -> Int#
foreign import prim "expFloat#" expFloat# :: Float# -> Float#
foreign import prim "expm1Float#" expm1Float# :: Float# -> Float#
foreign import prim "logFloat#" logFloat# :: Float# -> Float#
foreign import prim "log1pFloat#" log1pFloat# :: Float# -> Float#
foreign import prim "sqrtFloat#" sqrtFloat# :: Float# -> Float#
foreign import prim "sinFloat#" sinFloat# :: Float# -> Float#
foreign import prim "cosFloat#" cosFloat# :: Float# -> Float#
foreign import prim "tanFloat#" tanFloat# :: Float# -> Float#
foreign import prim "asinFloat#" asinFloat# :: Float# -> Float#
foreign import prim "acosFloat#" acosFloat# :: Float# -> Float#
foreign import prim "atanFloat#" atanFloat# :: Float# -> Float#
foreign import prim "sinhFloat#" sinhFloat# :: Float# -> Float#
foreign import prim "coshFloat#" coshFloat# :: Float# -> Float#
foreign import prim "tanhFloat#" tanhFloat# :: Float# -> Float#
foreign import prim "asinhFloat#" asinhFloat# :: Float# -> Float#
foreign import prim "acoshFloat#" acoshFloat# :: Float# -> Float#
foreign import prim "atanhFloat#" atanhFloat# :: Float# -> Float#
foreign import prim "powerFloat#" powerFloat# :: Float# -> Float# -> Float#
foreign import prim "float2Double#" float2Double# :: Float# -> Double#
foreign import prim "decodeFloat_Int#" decodeFloat_Int# :: Float# -> (# Int#, Int# #)
foreign import prim "castFloatToWord32#" castFloatToWord32# :: Float# -> Word32#
foreign import prim "castWord32ToFloat#" castWord32ToFloat# :: Word32# -> Float#
foreign import prim "fmaddFloat#" fmaddFloat# :: Float# -> Float# -> Float# -> Float#
foreign import prim "fmsubFloat#" fmsubFloat# :: Float# -> Float# -> Float# -> Float#
foreign import prim "fnmaddFloat#" fnmaddFloat# :: Float# -> Float# -> Float# -> Float#
foreign import prim "fnmsubFloat#" fnmsubFloat# :: Float# -> Float# -> Float# -> Float#
foreign import prim "fmaddDouble#" fmaddDouble# :: Double# -> Double# -> Double# -> Double#
foreign import prim "fmsubDouble#" fmsubDouble# :: Double# -> Double# -> Double# -> Double#
foreign import prim "fnmaddDouble#" fnmaddDouble# :: Double# -> Double# -> Double# -> Double#
foreign import prim "fnmsubDouble#" fnmsubDouble# :: Double# -> Double# -> Double# -> Double#
foreign import prim "newArray#" newArray# :: Int# -> a_levpoly -> State# s -> (# State# s, MutableArray# s a_levpoly #)
foreign import prim "readArray#" readArray# :: MutableArray# s a_levpoly -> Int# -> State# s -> (# State# s, a_levpoly #)
foreign import prim "writeArray#" writeArray# :: MutableArray# s a_levpoly -> Int# -> a_levpoly -> State# s -> State# s
foreign import prim "sizeofArray#" sizeofArray# :: Array# a_levpoly -> Int#
foreign import prim "sizeofMutableArray#" sizeofMutableArray# :: MutableArray# s a_levpoly -> Int#
foreign import prim "indexArray#" indexArray# :: Array# a_levpoly -> Int# -> (# a_levpoly #)
foreign import prim "unsafeFreezeArray#" unsafeFreezeArray# :: MutableArray# s a_levpoly -> State# s -> (# State# s, Array# a_levpoly #)
foreign import prim "unsafeThawArray#" unsafeThawArray# :: Array# a_levpoly -> State# s -> (# State# s, MutableArray# s a_levpoly #)
foreign import prim "copyArray#" copyArray# :: Array# a_levpoly -> Int# -> MutableArray# s a_levpoly -> Int# -> Int# -> State# s -> State# s
foreign import prim "copyMutableArray#" copyMutableArray# :: MutableArray# s a_levpoly -> Int# -> MutableArray# s a_levpoly -> Int# -> Int# -> State# s -> State# s
foreign import prim "cloneArray#" cloneArray# :: Array# a_levpoly -> Int# -> Int# -> Array# a_levpoly
foreign import prim "cloneMutableArray#" cloneMutableArray# :: MutableArray# s a_levpoly -> Int# -> Int# -> State# s -> (# State# s, MutableArray# s a_levpoly #)
foreign import prim "freezeArray#" freezeArray# :: MutableArray# s a_levpoly -> Int# -> Int# -> State# s -> (# State# s, Array# a_levpoly #)
foreign import prim "thawArray#" thawArray# :: Array# a_levpoly -> Int# -> Int# -> State# s -> (# State# s, MutableArray# s a_levpoly #)
foreign import prim "casArray#" casArray# :: MutableArray# s a_levpoly -> Int# -> a_levpoly -> a_levpoly -> State# s -> (# State# s, Int#, a_levpoly #)
foreign import prim "newSmallArray#" newSmallArray# :: Int# -> a_levpoly -> State# s -> (# State# s, SmallMutableArray# s a_levpoly #)
foreign import prim "shrinkSmallMutableArray#" shrinkSmallMutableArray# :: SmallMutableArray# s a_levpoly -> Int# -> State# s -> State# s
foreign import prim "readSmallArray#" readSmallArray# :: SmallMutableArray# s a_levpoly -> Int# -> State# s -> (# State# s, a_levpoly #)
foreign import prim "writeSmallArray#" writeSmallArray# :: SmallMutableArray# s a_levpoly -> Int# -> a_levpoly -> State# s -> State# s
foreign import prim "sizeofSmallArray#" sizeofSmallArray# :: SmallArray# a_levpoly -> Int#
foreign import prim "sizeofSmallMutableArray#" sizeofSmallMutableArray# :: SmallMutableArray# s a_levpoly -> Int#
foreign import prim "getSizeofSmallMutableArray#" getSizeofSmallMutableArray# :: SmallMutableArray# s a_levpoly -> State# s -> (# State# s, Int# #)
foreign import prim "indexSmallArray#" indexSmallArray# :: SmallArray# a_levpoly -> Int# -> (# a_levpoly #)
foreign import prim "unsafeFreezeSmallArray#" unsafeFreezeSmallArray# :: SmallMutableArray# s a_levpoly -> State# s -> (# State# s, SmallArray# a_levpoly #)
foreign import prim "unsafeThawSmallArray#" unsafeThawSmallArray# :: SmallArray# a_levpoly -> State# s -> (# State# s, SmallMutableArray# s a_levpoly #)
foreign import prim "copySmallArray#" copySmallArray# :: SmallArray# a_levpoly -> Int# -> SmallMutableArray# s a_levpoly -> Int# -> Int# -> State# s -> State# s
foreign import prim "copySmallMutableArray#" copySmallMutableArray# :: SmallMutableArray# s a_levpoly -> Int# -> SmallMutableArray# s a_levpoly -> Int# -> Int# -> State# s -> State# s
foreign import prim "cloneSmallArray#" cloneSmallArray# :: SmallArray# a_levpoly -> Int# -> Int# -> SmallArray# a_levpoly
foreign import prim "cloneSmallMutableArray#" cloneSmallMutableArray# :: SmallMutableArray# s a_levpoly -> Int# -> Int# -> State# s -> (# State# s, SmallMutableArray# s a_levpoly #)
foreign import prim "freezeSmallArray#" freezeSmallArray# :: SmallMutableArray# s a_levpoly -> Int# -> Int# -> State# s -> (# State# s, SmallArray# a_levpoly #)
foreign import prim "thawSmallArray#" thawSmallArray# :: SmallArray# a_levpoly -> Int# -> Int# -> State# s -> (# State# s, SmallMutableArray# s a_levpoly #)
foreign import prim "casSmallArray#" casSmallArray# :: SmallMutableArray# s a_levpoly -> Int# -> a_levpoly -> a_levpoly -> State# s -> (# State# s, Int#, a_levpoly #)
foreign import prim "newByteArray#" newByteArray# :: Int# -> State# s -> (# State# s, MutableByteArray# s #)
foreign import prim "newPinnedByteArray#" newPinnedByteArray# :: Int# -> State# s -> (# State# s, MutableByteArray# s #)
foreign import prim "newAlignedPinnedByteArray#" newAlignedPinnedByteArray# :: Int# -> Int# -> State# s -> (# State# s, MutableByteArray# s #)
foreign import prim "isMutableByteArrayPinned#" isMutableByteArrayPinned# :: MutableByteArray# s -> Int#
foreign import prim "isByteArrayPinned#" isByteArrayPinned# :: ByteArray# -> Int#
foreign import prim "isByteArrayWeaklyPinned#" isByteArrayWeaklyPinned# :: ByteArray# -> Int#
foreign import prim "isMutableByteArrayWeaklyPinned#" isMutableByteArrayWeaklyPinned# :: MutableByteArray# s -> Int#
foreign import prim "byteArrayContents#" byteArrayContents# :: ByteArray# -> Addr#
foreign import prim "mutableByteArrayContents#" mutableByteArrayContents# :: MutableByteArray# s -> Addr#
foreign import prim "shrinkMutableByteArray#" shrinkMutableByteArray# :: MutableByteArray# s -> Int# -> State# s -> State# s
foreign import prim "resizeMutableByteArray#" resizeMutableByteArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s,MutableByteArray# s #)
foreign import prim "unsafeFreezeByteArray#" unsafeFreezeByteArray# :: MutableByteArray# s -> State# s -> (# State# s, ByteArray# #)
foreign import prim "unsafeThawByteArray#" unsafeThawByteArray# :: ByteArray# -> State# s -> (# State# s, MutableByteArray# s #)
foreign import prim "sizeofByteArray#" sizeofByteArray# :: ByteArray# -> Int#
foreign import prim "sizeofMutableByteArray#" sizeofMutableByteArray# :: MutableByteArray# s -> Int#
foreign import prim "getSizeofMutableByteArray#" getSizeofMutableByteArray# :: MutableByteArray# s -> State# s -> (# State# s, Int# #)
foreign import prim "compareByteArrays#" compareByteArrays# :: ByteArray# -> Int# -> ByteArray# -> Int# -> Int# -> Int#
foreign import prim "copyByteArray#" copyByteArray# :: ByteArray# -> Int# -> MutableByteArray# s -> Int# -> Int# -> State# s -> State# s
foreign import prim "copyMutableByteArray#" copyMutableByteArray# :: MutableByteArray# s -> Int# -> MutableByteArray# s -> Int# -> Int# -> State# s -> State# s
foreign import prim "copyMutableByteArrayNonOverlapping#" copyMutableByteArrayNonOverlapping# :: MutableByteArray# s -> Int# -> MutableByteArray# s -> Int# -> Int# -> State# s -> State# s
foreign import prim "copyByteArrayToAddr#" copyByteArrayToAddr# :: ByteArray# -> Int# -> Addr# -> Int# -> State# s -> State# s
foreign import prim "copyMutableByteArrayToAddr#" copyMutableByteArrayToAddr# :: MutableByteArray# s -> Int# -> Addr# -> Int# -> State# s -> State# s
foreign import prim "copyAddrToByteArray#" copyAddrToByteArray# :: Addr# -> MutableByteArray# s -> Int# -> Int# -> State# s -> State# s
foreign import prim "copyAddrToAddr#" copyAddrToAddr# :: Addr# -> Addr# -> Int# -> State# RealWorld -> State# RealWorld
foreign import prim "copyAddrToAddrNonOverlapping#" copyAddrToAddrNonOverlapping# :: Addr# -> Addr# -> Int# -> State# RealWorld -> State# RealWorld
foreign import prim "setByteArray#" setByteArray# :: MutableByteArray# s -> Int# -> Int# -> Int# -> State# s -> State# s
foreign import prim "setAddrRange#" setAddrRange# :: Addr# -> Int# -> Int# -> State# RealWorld -> State# RealWorld
foreign import prim "atomicReadIntArray#" atomicReadIntArray# :: MutableByteArray# s -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "atomicWriteIntArray#" atomicWriteIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> State# s
foreign import prim "casIntArray#" casIntArray# :: MutableByteArray# s -> Int# -> Int# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "casInt8Array#" casInt8Array# :: MutableByteArray# s -> Int# -> Int8# -> Int8# -> State# s -> (# State# s, Int8# #)
foreign import prim "casInt16Array#" casInt16Array# :: MutableByteArray# s -> Int# -> Int16# -> Int16# -> State# s -> (# State# s, Int16# #)
foreign import prim "casInt32Array#" casInt32Array# :: MutableByteArray# s -> Int# -> Int32# -> Int32# -> State# s -> (# State# s, Int32# #)
foreign import prim "casInt64Array#" casInt64Array# :: MutableByteArray# s -> Int# -> Int64# -> Int64# -> State# s -> (# State# s, Int64# #)
foreign import prim "fetchAddIntArray#" fetchAddIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "fetchSubIntArray#" fetchSubIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "fetchAndIntArray#" fetchAndIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "fetchNandIntArray#" fetchNandIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "fetchOrIntArray#" fetchOrIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "fetchXorIntArray#" fetchXorIntArray# :: MutableByteArray# s -> Int# -> Int# -> State# s -> (# State# s, Int# #)
foreign import prim "plusAddr#" plusAddr# :: Addr# -> Int# -> Addr#
foreign import prim "minusAddr#" minusAddr# :: Addr# -> Addr# -> Int#
foreign import prim "remAddr#" remAddr# :: Addr# -> Int# -> Int#
foreign import prim "addr2Int#" addr2Int# :: Addr# -> Int#
foreign import prim "int2Addr#" int2Addr# :: Int# -> Addr#
foreign import prim "gtAddr#" gtAddr# :: Addr# -> Addr# -> Int#
foreign import prim "geAddr#" geAddr# :: Addr# -> Addr# -> Int#
foreign import prim "eqAddr#" eqAddr# :: Addr# -> Addr# -> Int#
foreign import prim "neAddr#" neAddr# :: Addr# -> Addr# -> Int#
foreign import prim "ltAddr#" ltAddr# :: Addr# -> Addr# -> Int#
foreign import prim "leAddr#" leAddr# :: Addr# -> Addr# -> Int#
foreign import prim "atomicExchangeAddrAddr#" atomicExchangeAddrAddr# :: Addr# -> Addr# -> State# s -> (# State# s, Addr# #)
foreign import prim "atomicExchangeWordAddr#" atomicExchangeWordAddr# :: Addr# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "atomicCasAddrAddr#" atomicCasAddrAddr# :: Addr# -> Addr# -> Addr# -> State# s -> (# State# s, Addr# #)
foreign import prim "atomicCasWordAddr#" atomicCasWordAddr# :: Addr# -> Word# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "atomicCasWord8Addr#" atomicCasWord8Addr# :: Addr# -> Word8# -> Word8# -> State# s -> (# State# s, Word8# #)
foreign import prim "atomicCasWord16Addr#" atomicCasWord16Addr# :: Addr# -> Word16# -> Word16# -> State# s -> (# State# s, Word16# #)
foreign import prim "atomicCasWord32Addr#" atomicCasWord32Addr# :: Addr# -> Word32# -> Word32# -> State# s -> (# State# s, Word32# #)
foreign import prim "atomicCasWord64Addr#" atomicCasWord64Addr# :: Addr# -> Word64# -> Word64# -> State# s -> (# State# s, Word64# #)
foreign import prim "fetchAddWordAddr#" fetchAddWordAddr# :: Addr# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "fetchSubWordAddr#" fetchSubWordAddr# :: Addr# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "fetchAndWordAddr#" fetchAndWordAddr# :: Addr# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "fetchNandWordAddr#" fetchNandWordAddr# :: Addr# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "fetchOrWordAddr#" fetchOrWordAddr# :: Addr# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "fetchXorWordAddr#" fetchXorWordAddr# :: Addr# -> Word# -> State# s -> (# State# s, Word# #)
foreign import prim "atomicReadWordAddr#" atomicReadWordAddr# :: Addr# -> State# s -> (# State# s, Word# #)
foreign import prim "atomicWriteWordAddr#" atomicWriteWordAddr# :: Addr# -> Word# -> State# s -> State# s
foreign import prim "newMutVar#" newMutVar# :: a_levpoly -> State# s -> (# State# s, MutVar# s a_levpoly #)
foreign import prim "readMutVar#" readMutVar# :: MutVar# s a_levpoly -> State# s -> (# State# s, a_levpoly #)
foreign import prim "writeMutVar#" writeMutVar# :: MutVar# s a_levpoly -> a_levpoly -> State# s -> State# s
foreign import prim "atomicSwapMutVar#" atomicSwapMutVar# :: MutVar# s a_levpoly -> a_levpoly -> State# s -> (# State# s, a_levpoly #)
foreign import prim "atomicModifyMutVar2#" atomicModifyMutVar2# :: MutVar# s a -> (a -> c) -> State# s -> (# State# s, a, c #)
foreign import prim "atomicModifyMutVar_#" atomicModifyMutVar_# :: MutVar# s a -> (a -> a) -> State# s -> (# State# s, a, a #)
foreign import prim "casMutVar#" casMutVar# :: MutVar# s a_levpoly -> a_levpoly -> a_levpoly -> State# s -> (# State# s, Int#, a_levpoly #)
foreign import prim "catch#" catch# :: (State# RealWorld -> (# State# RealWorld, a_reppoly #) ) -> (b_levpoly -> State# RealWorld -> (# State# RealWorld, a_reppoly #) ) -> State# RealWorld -> (# State# RealWorld, a_reppoly #)
foreign import prim "raise#" raise# :: a_levpoly -> b_reppoly
foreign import prim "raiseUnderflow#" raiseUnderflow# :: (# #) -> b_reppoly
foreign import prim "raiseOverflow#" raiseOverflow# :: (# #) -> b_reppoly
foreign import prim "raiseDivZero#" raiseDivZero# :: (# #) -> b_reppoly
foreign import prim "raiseIO#" raiseIO# :: a_levpoly -> State# RealWorld -> (# State# RealWorld, b_reppoly #)
foreign import prim "maskAsyncExceptions#" maskAsyncExceptions# :: (State# RealWorld -> (# State# RealWorld, a_reppoly #)) -> (State# RealWorld -> (# State# RealWorld, a_reppoly #))
foreign import prim "maskUninterruptible#" maskUninterruptible# :: (State# RealWorld -> (# State# RealWorld, a_reppoly #)) -> (State# RealWorld -> (# State# RealWorld, a_reppoly #))
foreign import prim "unmaskAsyncExceptions#" unmaskAsyncExceptions# :: (State# RealWorld -> (# State# RealWorld, a_reppoly #)) -> (State# RealWorld -> (# State# RealWorld, a_reppoly #))
foreign import prim "getMaskingState#" getMaskingState# :: State# RealWorld -> (# State# RealWorld, Int# #)
foreign import prim "newPromptTag#" newPromptTag# :: State# RealWorld -> (# State# RealWorld, PromptTag# a #)
foreign import prim "prompt#" prompt# :: PromptTag# a -> (State# RealWorld -> (# State# RealWorld, a #)) -> State# RealWorld -> (# State# RealWorld, a #)
foreign import prim "control0#" control0# :: PromptTag# a -> (((State# RealWorld -> (# State# RealWorld, b_reppoly #)) -> State# RealWorld -> (# State# RealWorld, a #)) -> State# RealWorld -> (# State# RealWorld, a #)) -> State# RealWorld -> (# State# RealWorld, b_reppoly #)
foreign import prim "atomically#" atomically# :: (State# RealWorld -> (# State# RealWorld, a_levpoly #) ) -> State# RealWorld -> (# State# RealWorld, a_levpoly #)
foreign import prim "retry#" retry# :: State# RealWorld -> (# State# RealWorld, a_levpoly #)
foreign import prim "catchRetry#" catchRetry# :: (State# RealWorld -> (# State# RealWorld, a_levpoly #) ) -> (State# RealWorld -> (# State# RealWorld, a_levpoly #) ) -> (State# RealWorld -> (# State# RealWorld, a_levpoly #) )
foreign import prim "catchSTM#" catchSTM# :: (State# RealWorld -> (# State# RealWorld, a_levpoly #) ) -> (b -> State# RealWorld -> (# State# RealWorld, a_levpoly #) ) -> (State# RealWorld -> (# State# RealWorld, a_levpoly #) )
foreign import prim "newTVar#" newTVar# :: a_levpoly -> State# s -> (# State# s, TVar# s a_levpoly #)
foreign import prim "readTVar#" readTVar# :: TVar# s a_levpoly -> State# s -> (# State# s, a_levpoly #)
foreign import prim "readTVarIO#" readTVarIO# :: TVar# s a_levpoly -> State# s -> (# State# s, a_levpoly #)
foreign import prim "writeTVar#" writeTVar# :: TVar# s a_levpoly -> a_levpoly -> State# s -> State# s
foreign import prim "newMVar#" newMVar# :: State# s -> (# State# s, MVar# s a_levpoly #)
foreign import prim "takeMVar#" takeMVar# :: MVar# s a_levpoly -> State# s -> (# State# s, a_levpoly #)
foreign import prim "tryTakeMVar#" tryTakeMVar# :: MVar# s a_levpoly -> State# s -> (# State# s, Int#, a_levpoly #)
foreign import prim "putMVar#" putMVar# :: MVar# s a_levpoly -> a_levpoly -> State# s -> State# s
foreign import prim "tryPutMVar#" tryPutMVar# :: MVar# s a_levpoly -> a_levpoly -> State# s -> (# State# s, Int# #)
foreign import prim "readMVar#" readMVar# :: MVar# s a_levpoly -> State# s -> (# State# s, a_levpoly #)
foreign import prim "tryReadMVar#" tryReadMVar# :: MVar# s a_levpoly -> State# s -> (# State# s, Int#, a_levpoly #)
foreign import prim "isEmptyMVar#" isEmptyMVar# :: MVar# s a_levpoly -> State# s -> (# State# s, Int# #)
foreign import prim "newIOPort#" newIOPort# :: State# s -> (# State# s, IOPort# s a_levpoly #)
foreign import prim "readIOPort#" readIOPort# :: IOPort# s a_levpoly -> State# s -> (# State# s, a_levpoly #)
foreign import prim "writeIOPort#" writeIOPort# :: IOPort# s a_levpoly -> a_levpoly -> State# s -> (# State# s, Int# #)
foreign import prim "delay#" delay# :: Int# -> State# s -> State# s
foreign import prim "waitRead#" waitRead# :: Int# -> State# s -> State# s
foreign import prim "waitWrite#" waitWrite# :: Int# -> State# s -> State# s
foreign import prim "fork#" fork# :: (State# RealWorld -> (# State# RealWorld, a_reppoly #)) -> State# RealWorld -> (# State# RealWorld, ThreadId# #)
foreign import prim "forkOn#" forkOn# :: Int# -> (State# RealWorld -> (# State# RealWorld, a_reppoly #)) -> State# RealWorld -> (# State# RealWorld, ThreadId# #)
foreign import prim "killThread#" killThread# :: ThreadId# -> a -> State# RealWorld -> State# RealWorld
foreign import prim "yield#" yield# :: State# RealWorld -> State# RealWorld
foreign import prim "myThreadId#" myThreadId# :: State# RealWorld -> (# State# RealWorld, ThreadId# #)
foreign import prim "labelThread#" labelThread# :: ThreadId# -> ByteArray# -> State# RealWorld -> State# RealWorld
foreign import prim "isCurrentThreadBound#" isCurrentThreadBound# :: State# RealWorld -> (# State# RealWorld, Int# #)
foreign import prim "noDuplicate#" noDuplicate# :: State# s -> State# s
foreign import prim "threadLabel#" threadLabel# :: ThreadId# -> State# RealWorld -> (# State# RealWorld, Int#, ByteArray# #)
foreign import prim "threadStatus#" threadStatus# :: ThreadId# -> State# RealWorld -> (# State# RealWorld, Int#, Int#, Int# #)
foreign import prim "listThreads#" listThreads# :: State# RealWorld -> (# State# RealWorld, Array# ThreadId# #)
foreign import prim "mkWeak#" mkWeak# :: a_levpoly -> b_levpoly -> (State# RealWorld -> (# State# RealWorld, c #)) -> State# RealWorld -> (# State# RealWorld, Weak# b_levpoly #)
foreign import prim "mkWeakNoFinalizer#" mkWeakNoFinalizer# :: a_levpoly -> b_levpoly -> State# RealWorld -> (# State# RealWorld, Weak# b_levpoly #)
foreign import prim "addCFinalizerToWeak#" addCFinalizerToWeak# :: Addr# -> Addr# -> Int# -> Addr# -> Weak# b_levpoly -> State# RealWorld -> (# State# RealWorld, Int# #)
foreign import prim "deRefWeak#" deRefWeak# :: Weak# a_levpoly -> State# RealWorld -> (# State# RealWorld, Int#, a_levpoly #)
foreign import prim "finalizeWeak#" finalizeWeak# :: Weak# a_levpoly -> State# RealWorld -> (# State# RealWorld, Int#, (State# RealWorld -> (# State# RealWorld, b #) ) #)
foreign import prim "touch#" touch# :: a_levpoly -> State# s -> State# s
foreign import prim "makeStablePtr#" makeStablePtr# :: a_levpoly -> State# RealWorld -> (# State# RealWorld, StablePtr# a_levpoly #)
foreign import prim "deRefStablePtr#" deRefStablePtr# :: StablePtr# a_levpoly -> State# RealWorld -> (# State# RealWorld, a_levpoly #)
foreign import prim "eqStablePtr#" eqStablePtr# :: StablePtr# a_levpoly -> StablePtr# a_levpoly -> Int#
foreign import prim "makeStableName#" makeStableName# :: a_levpoly -> State# RealWorld -> (# State# RealWorld, StableName# a_levpoly #)
foreign import prim "stableNameToInt#" stableNameToInt# :: StableName# a_levpoly -> Int#
foreign import prim "compactNew#" compactNew# :: Word# -> State# RealWorld -> (# State# RealWorld, Compact# #)
foreign import prim "compactResize#" compactResize# :: Compact# -> Word# -> State# RealWorld -> State# RealWorld
foreign import prim "compactContains#" compactContains# :: Compact# -> a -> State# RealWorld -> (# State# RealWorld, Int# #)
foreign import prim "compactContainsAny#" compactContainsAny# :: a -> State# RealWorld -> (# State# RealWorld, Int# #)
foreign import prim "compactGetFirstBlock#" compactGetFirstBlock# :: Compact# -> State# RealWorld -> (# State# RealWorld, Addr#, Word# #)
foreign import prim "compactGetNextBlock#" compactGetNextBlock# :: Compact# -> Addr# -> State# RealWorld -> (# State# RealWorld, Addr#, Word# #)
foreign import prim "compactAllocateBlock#" compactAllocateBlock# :: Word# -> Addr# -> State# RealWorld -> (# State# RealWorld, Addr# #)
foreign import prim "compactFixupPointers#" compactFixupPointers# :: Addr# -> Addr# -> State# RealWorld -> (# State# RealWorld, Compact#, Addr# #)
foreign import prim "compactAdd#" compactAdd# :: Compact# -> a -> State# RealWorld -> (# State# RealWorld, a #)
foreign import prim "compactAddWithSharing#" compactAddWithSharing# :: Compact# -> a -> State# RealWorld -> (# State# RealWorld, a #)
foreign import prim "compactSize#" compactSize# :: Compact# -> State# RealWorld -> (# State# RealWorld, Word# #)
foreign import prim "reallyUnsafePtrEquality#" reallyUnsafePtrEquality# :: a_levpoly -> b_levpoly -> Int#
foreign import prim "par#" par# :: a -> Int#
foreign import prim "spark#" spark# :: a -> State# s -> (# State# s, a #)
foreign import prim "getSpark#" getSpark# :: State# s -> (# State# s, Int#, a #)
foreign import prim "numSparks#" numSparks# :: State# s -> (# State# s, Int# #)
foreign import prim "keepAlive#" keepAlive# :: a_levpoly -> State# s -> (State# s -> b_reppoly) -> b_reppoly
foreign import prim "dataToTagSmall#" dataToTagSmall# :: a_levpoly -> Int#
foreign import prim "dataToTagLarge#" dataToTagLarge# :: a_levpoly -> Int#
foreign import prim "tagToEnum#" tagToEnum# :: Int# -> a
foreign import prim "addrToAny#" addrToAny# :: Addr# -> (# a_levpoly #)
foreign import prim "anyToAddr#" anyToAddr# :: a -> State# RealWorld -> (# State# RealWorld, Addr# #)
foreign import prim "newBCO#" newBCO# :: ByteArray# -> ByteArray# -> Array# a -> Int# -> ByteArray# -> State# s -> (# State# s, BCO #)
foreign import prim "unpackClosure#" unpackClosure# :: a -> (# Addr#, ByteArray#, Array# b #)
foreign import prim "closureSize#" closureSize# :: a -> Int#
foreign import prim "getApStackVal#" getApStackVal# :: a -> Int# -> (# Int#, b #)
foreign import prim "getCCSOf#" getCCSOf# :: a -> State# s -> (# State# s, Addr# #)
foreign import prim "getCurrentCCS#" getCurrentCCS# :: a -> State# s -> (# State# s, Addr# #)
foreign import prim "clearCCS#" clearCCS# :: (State# s -> (# State# s, a #)) -> State# s -> (# State# s, a #)
foreign import prim "whereFrom#" whereFrom# :: a -> Addr# -> State# s -> (# State# s, Int# #)
foreign import prim "traceEvent#" traceEvent# :: Addr# -> State# s -> State# s
foreign import prim "traceBinaryEvent#" traceBinaryEvent# :: Addr# -> Int# -> State# s -> State# s
foreign import prim "traceMarker#" traceMarker# :: Addr# -> State# s -> State# s
foreign import prim "setThreadAllocationCounter#" setThreadAllocationCounter# :: Int64# -> State# RealWorld -> State# RealWorld
foreign import prim "prefetchByteArray3#" prefetchByteArray3# :: ByteArray# -> Int# ->  State# s -> State# s
foreign import prim "prefetchMutableByteArray3#" prefetchMutableByteArray3# :: MutableByteArray# s -> Int# -> State# s -> State# s
foreign import prim "prefetchAddr3#" prefetchAddr3# :: Addr# -> Int# -> State# s -> State# s
foreign import prim "prefetchValue3#" prefetchValue3# :: a -> State# s -> State# s
foreign import prim "prefetchByteArray2#" prefetchByteArray2# :: ByteArray# -> Int# ->  State# s -> State# s
foreign import prim "prefetchMutableByteArray2#" prefetchMutableByteArray2# :: MutableByteArray# s -> Int# -> State# s -> State# s
foreign import prim "prefetchAddr2#" prefetchAddr2# :: Addr# -> Int# ->  State# s -> State# s
foreign import prim "prefetchValue2#" prefetchValue2# :: a ->  State# s -> State# s
foreign import prim "prefetchByteArray1#" prefetchByteArray1# :: ByteArray# -> Int# -> State# s -> State# s
foreign import prim "prefetchMutableByteArray1#" prefetchMutableByteArray1# :: MutableByteArray# s -> Int# -> State# s -> State# s
foreign import prim "prefetchAddr1#" prefetchAddr1# :: Addr# -> Int# -> State# s -> State# s
foreign import prim "prefetchValue1#" prefetchValue1# :: a -> State# s -> State# s
foreign import prim "prefetchByteArray0#" prefetchByteArray0# :: ByteArray# -> Int# ->  State# s -> State# s
foreign import prim "prefetchMutableByteArray0#" prefetchMutableByteArray0# :: MutableByteArray# s -> Int# -> State# s -> State# s
foreign import prim "prefetchAddr0#" prefetchAddr0# :: Addr# -> Int# -> State# s -> State# s
foreign import prim "prefetchValue0#" prefetchValue0# :: a -> State# s -> State# s
infixl 6 +#, -#, +##, -##
infixl 7 *#, *##, /##
infix 4 ==#, /=#, <#, <=#, >#, >=#, ==##, /=##, <##, <=##, >##, >=##
