{-# LANGUAGE ScopedTypeVariables #-}

f :: forall a. forall b. a -> b -> b
f _ x = x :: b

main :: IO ()
main = print (f () False)
