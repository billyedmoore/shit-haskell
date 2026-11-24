{-# LANGUAGE TypeFamilies #-}

type family Pick a
data family Result a
data instance Result (Pick a) = Item

main = pure ()
