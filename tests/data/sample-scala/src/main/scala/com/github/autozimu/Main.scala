package com.github.autozimu

import com.typesafe.scalalogging.LazyLogging

object Main extends App with LazyLogging {

  def helloWorld(name: String): String = {
    "Hello " + name + "!"
  }

  logger.info(helloWorld("nabezokodaikokn"))
}
