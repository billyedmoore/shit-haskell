{-# LANGUAGE RankNTypes #-}

f :: forall a. (forall b. b -> c) -> a -> a
f _ x = x

main :: IO ()
main = pure ()
