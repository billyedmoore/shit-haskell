{-# LANGUAGE ExplicitForAll, TypeAbstractions #-}
f :: forall _. Int
f = 3
main :: IO ()
main = print f
