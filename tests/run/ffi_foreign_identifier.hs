{-# LANGUAGE NoForeignFunctionInterface #-}

{-# INLINE foreign #-}
foreign :: Int
foreign = 3

main :: IO ()
main = print foreign
