class C a where
  m :: a -> b -> b
instance C Bool where
  m _ = id
main :: IO ()
main = print (m True (17 :: Int))
