{-# LANGUAGE ExplicitForAll, NoScopedTypeVariables #-}
class C a where
  m :: forall b. a -> b -> b
instance C Bool where
  m _ = id
main :: IO ()
main = print (m True (17 :: Int))
