{-# LANGUAGE ForeignFunctionInterface #-}
import Foreign.C.Types
import Foreign.Ptr

foreign import ccall "&abs" c_abs_ptr :: FunPtr (CInt -> CInt)
foreign import ccall "dynamic" mkAbs :: FunPtr (CInt -> CInt) -> CInt -> CInt

main :: IO ()
main = do
  let f = mkAbs c_abs_ptr
  print (f (-42), f 0, f 17)
