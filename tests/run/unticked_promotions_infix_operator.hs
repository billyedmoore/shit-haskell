{-# LANGUAGE DataKinds #-}

data D = D :*: D | Leaf
type T = 'Leaf :*: 'Leaf

main :: IO ()
main = pure ()
