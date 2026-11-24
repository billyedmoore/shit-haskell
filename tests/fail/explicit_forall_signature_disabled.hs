{-# LANGUAGE Haskell2010 #-}

identity :: forall a. a -> a
identity = id

main :: IO ()
main = pure ()
