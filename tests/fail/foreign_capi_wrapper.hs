{-# LANGUAGE CApiFFI #-}

import Foreign.Ptr (FunPtr)

foreign import capi "wrapper" wrap :: (Int -> Int) -> IO (FunPtr (Int -> Int))

main = pure ()
