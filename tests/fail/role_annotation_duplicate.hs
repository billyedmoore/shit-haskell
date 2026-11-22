{-# LANGUAGE RoleAnnotations #-}

type role Box representational
type role Box nominal
data Box a = Box a

main = pure ()
