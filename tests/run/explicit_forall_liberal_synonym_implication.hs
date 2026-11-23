{-# LANGUAGE Haskell2010, LiberalTypeSynonyms #-}

identity :: forall a. a -> a
identity = id

main :: IO ()
main = print (identity (7 :: Int))
