{-# LANGUAGE NoScopedTypeVariables #-}

f :: forall a. a -> a
f x = let g :: forall b. b -> a
          g _ = x
      in g ()

main :: IO ()
main = print (f False)
