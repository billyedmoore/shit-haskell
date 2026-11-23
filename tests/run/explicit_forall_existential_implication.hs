{-# LANGUAGE Haskell2010, ExistentialQuantification #-}

identity :: forall a. a -> a
identity = id

main :: IO ()
main = print (identity (7 :: Int))
