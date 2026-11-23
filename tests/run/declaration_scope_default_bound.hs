{-# LANGUAGE ExplicitForAll, DefaultSignatures #-}
class C a where
  m :: a -> b -> b
  default m :: forall b. a -> b -> b
  m _ = id
instance C Bool
main :: IO ()
main = print (m True (17 :: Int))
