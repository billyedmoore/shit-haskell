{-# LANGUAGE ScopedTypeVariables #-}

f :: forall a. a -> a
f x = id @a x

main :: IO ()
main = print (f False)
